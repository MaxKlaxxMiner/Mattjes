//! Syzygy's compression scheme in an own container (the port of
//! mattjesGo/pairs): a stream of small values is rewritten by recursive pair
//! replacement, the symbols get a canonical Huffman code, the code is cut
//! into small fixed-size blocks with a position index; one value costs the
//! decoding of one block. The packer is deterministic: Go and Rust write
//! identical containers on any number of workers (fixed chunk borders).
//!
//! Container (little endian): a 32-byte header, u16 symbol counts per code
//! length (min_len..max_len), 3 bytes per symbol (two 12-bit symbol numbers,
//! or the value and 0xfff for a leaf; symbols are numbered by code length,
//! the uncoded ones last), the index (6 bytes per 2^idx_bits positions:
//! block u32, offset u16), the size table (u16 per block: positions minus
//! one), the blocks, 8 zero bytes.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Tuning of the packer; zero values take the defaults (see the Go package).
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub block_bytes: usize,
    pub index_bits: usize,
    pub max_symbols: usize,
    pub max_symbol_len: usize,
    pub min_count: usize,
    pub pairs_per_pass: usize,
    /// The dictionary from evenly spaced windows of about this many values
    /// instead of the whole stream, which is then tokenised in one pass; 0 =
    /// the whole stream (streams of up to twice the sample always).
    pub dict_sample: usize,
    pub workers: usize,
}

impl Options {
    fn with_defaults(mut self) -> Options {
        if self.max_symbols == 0 {
            self.max_symbols = 4095;
        }
        if self.max_symbol_len == 0 {
            self.max_symbol_len = 256;
        }
        if self.min_count == 0 {
            self.min_count = 8;
        }
        if self.pairs_per_pass == 0 {
            self.pairs_per_pass = 64;
        }
        if self.workers == 0 {
            self.workers = 1;
        }
        self.max_symbols = self.max_symbols.min(LEAF_MARK as usize - 1);
        self.max_symbol_len = self.max_symbol_len.min(POSITIONS_PER_BLOCK - 1);
        self
    }
}

const MAGIC: &[u8; 4] = b"MPR2";
const HEADER_BYTES: usize = 32;
/// The longest Huffman code (the decoder refills its 64-bit register 32 bits at a time).
pub const MAX_CODE_LEN: usize = 32;
const POSITIONS_PER_BLOCK: usize = 1 << 16;
const TAIL_BYTES: usize = 8;
const LEAF_MARK: u32 = 0xfff;
const CHUNK_SYMS: usize = 1 << 16;
/// The window of the dictionary sample (dict_sample).
const DICT_WINDOW: usize = 1 << 16;

/// Concatenates evenly spaced windows of the values, about target values in all.
fn sample_windows(values: &[u8], target: usize, window: usize) -> Vec<u8> {
    let n = values.len();
    let count = (target / window).max(1);
    if window * count >= n {
        return values.to_vec();
    }
    let mut out = Vec::with_capacity(window * count);
    for w in 0..count {
        let start = if count > 1 { w * (n - window) / (count - 1) } else { 0 };
        out.extend_from_slice(&values[start..start + window]);
    }
    out
}

/// What a packed stream looks like.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub positions: usize,
    pub symbols: usize,
    pub tokens: usize,
    pub blocks: usize,
    pub block_bytes: usize,
    pub index_bytes: usize,
    pub size_bytes: usize,
    pub code_bytes: usize,
    pub bytes: usize,
    pub min_code_len: usize,
    pub max_code_len: usize,
    pub index_bits: usize,
    pub passes: usize,
}

impl std::fmt::Display for Stats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} bytes: {} positions in {} tokens, {} symbols, {} blocks of {}, index {} (2^{}), sizes {}, code {}, codes {}..{} bits, {} passes",
            self.bytes, self.positions, self.tokens, self.symbols, self.blocks, self.block_bytes, self.index_bytes, self.index_bits,
            self.size_bytes, self.code_bytes, self.min_code_len, self.max_code_len, self.passes
        )
    }
}

// --- packing ---

struct Encoder {
    alphabet: usize,
    syms: Vec<u16>,
    pat: Vec<[u16; 2]>,
    sym_len: Vec<usize>,
    passes: usize,
}

/// Compresses values (each below alphabet, at most 256) into a container.
pub fn pack(values: &[u8], alphabet: usize, opt: Options) -> Vec<u8> {
    pack_stats(values, alphabet, opt).0
}

/// pack with the statistics of the result.
pub fn pack_stats(values: &[u8], alphabet: usize, opt: Options) -> (Vec<u8>, Stats) {
    let mut opt = opt.with_defaults();
    assert!((1..=256).contains(&alphabet), "pairs: alphabet must be 1..256");
    for (i, &v) in values.iter().enumerate() {
        assert!((v as usize) < alphabet, "pairs: value {} at {} beyond the alphabet {}", v, i, alphabet);
    }
    // the dictionary comes from the whole stream, or from windows of it
    // (dict_sample): then the stream is tokenised afterwards in one pass
    let sampled = opt.dict_sample > 0 && values.len() > 2 * opt.dict_sample;
    let window_sample;
    let source: &[u8] = if sampled {
        window_sample = sample_windows(values, opt.dict_sample, DICT_WINDOW);
        // a pair that occurs min_count times in the stream shows up fewer
        // times in the sample: the threshold shrinks with it (at least 2)
        opt.min_count = (opt.min_count * window_sample.len() / values.len()).max(2);
        &window_sample
    } else {
        values
    };
    let mut e = Encoder { alphabet, syms: source.iter().map(|&v| v as u16).collect(), pat: Vec::with_capacity(opt.max_symbols), sym_len: Vec::with_capacity(opt.max_symbols), passes: 0 };
    e.pat.resize(alphabet, [0, 0]);
    e.sym_len.resize(alphabet, 1);
    while e.sym_len.len() < opt.max_symbols {
        let counts = e.count_pairs(opt.workers);
        let chosen = e.choose(&counts, &opt);
        if chosen.is_empty() {
            break;
        }
        e.replace(&chosen, opt.workers);
        e.passes += 1;
    }
    let tokens = if sampled { e.tokenize(values, opt.workers) } else { std::mem::take(&mut e.syms) };
    let c = e.code(&tokens);
    if opt.block_bytes > 0 {
        return c.container(opt.block_bytes, opt.index_bits);
    }
    let (out32, st32) = c.container(32, opt.index_bits);
    let (out64, st64) = c.container(64, opt.index_bits);
    if out64.len() < out32.len() {
        (out64, st64)
    } else {
        (out32, st32)
    }
}

/// Open addressing from a pair (left<<16 | right) to its count; 0 = empty.
struct PairCounts {
    keys: Vec<u32>,
    counts: Vec<u32>,
    used: usize,
    bits: u32,
}

impl PairCounts {
    fn new(bits: u32) -> PairCounts {
        PairCounts { keys: vec![0; 1 << bits], counts: vec![0; 1 << bits], used: 0, bits }
    }

    fn add(&mut self, key: u32, n: u32) {
        if self.used * 10 > self.keys.len() * 6 {
            self.grow();
        }
        let mask = (self.keys.len() - 1) as u32;
        let mut h = key.wrapping_mul(0x9E37_79B1) >> (32 - self.bits);
        loop {
            let i = h as usize;
            if self.counts[i] == 0 {
                self.keys[i] = key;
                self.counts[i] = n;
                self.used += 1;
                return;
            }
            if self.keys[i] == key {
                self.counts[i] += n;
                return;
            }
            h = (h + 1) & mask;
        }
    }

    fn grow(&mut self) {
        let old_keys = std::mem::take(&mut self.keys);
        let old_counts = std::mem::take(&mut self.counts);
        self.bits += 1;
        self.keys = vec![0; 1 << self.bits];
        self.counts = vec![0; 1 << self.bits];
        self.used = 0;
        for (i, &n) in old_counts.iter().enumerate() {
            if n != 0 {
                self.add(old_keys[i], n);
            }
        }
    }
}

#[derive(Clone, Copy)]
struct PairChoice {
    left: u16,
    right: u16,
    count: u32,
}

/// Runs f over the fixed chunks of a stream of n symbols on the workers.
fn chunks(n: usize, workers: usize, f: &(dyn Fn(usize, usize, usize) + Sync)) {
    let num_chunks = n.div_ceil(CHUNK_SYMS);
    let workers = workers.max(1).min(num_chunks.max(1));
    let next = AtomicUsize::new(0);
    std::thread::scope(|s| {
        for w in 0..workers {
            let next = &next;
            s.spawn(move || loop {
                let c = next.fetch_add(1, Ordering::Relaxed);
                if c >= num_chunks {
                    return;
                }
                f(w, c * CHUNK_SYMS, n.min((c + 1) * CHUNK_SYMS));
            });
        }
    });
}

impl Encoder {
    /// Counts every adjacent pair. Every worker scans the whole stream and
    /// counts the pairs whose left symbol falls into its share, so the
    /// tables hold disjoint keys and nothing has to be merged.
    fn count_pairs(&self, workers: usize) -> Vec<PairCounts> {
        let n = self.syms.len();
        let workers = workers.max(1).min(n / (1 << 16) + 1);
        let syms = &self.syms;
        std::thread::scope(|s| {
            let handles: Vec<_> = (0..workers)
                .map(|w| {
                    s.spawn(move || {
                        let mut t = PairCounts::new(12);
                        for i in 0..n.saturating_sub(1) {
                            if syms[i] as usize % workers == w {
                                t.add((syms[i] as u32) << 16 | syms[i + 1] as u32, 1);
                            }
                        }
                        t
                    })
                })
                .collect();
            handles.into_iter().map(|h| h.join().unwrap()).collect()
        })
    }

    /// Picks the pairs of one pass: at least min_count occurrences, within
    /// max_symbol_len, pairwise disjoint in their symbols.
    fn choose(&self, tables: &[PairCounts], opt: &Options) -> Vec<PairChoice> {
        let mut cands = Vec::new();
        for counts in tables {
            for (i, &n) in counts.counts.iter().enumerate() {
                if (n as usize) < opt.min_count {
                    continue;
                }
                let k = counts.keys[i];
                let (l, r) = ((k >> 16) as u16, k as u16);
                if self.sym_len[l as usize] + self.sym_len[r as usize] > opt.max_symbol_len {
                    continue;
                }
                cands.push(PairChoice { left: l, right: r, count: n });
            }
        }
        cands.sort_by(|a, b| b.count.cmp(&a.count).then(a.left.cmp(&b.left)).then(a.right.cmp(&b.right)));
        let mut used = vec![false; self.sym_len.len()];
        let mut chosen = Vec::new();
        let room = opt.max_symbols - self.sym_len.len();
        for c in cands {
            if chosen.len() >= opt.pairs_per_pass || chosen.len() >= room {
                break;
            }
            if used[c.left as usize] || used[c.right as usize] {
                continue;
            }
            used[c.left as usize] = true;
            used[c.right as usize] = true;
            chosen.push(c);
        }
        chosen
    }

    /// Rewrites every occurrence of the chosen pairs by a new symbol, chunks
    /// compacted in place and stitched.
    fn replace(&mut self, chosen: &[PairChoice], workers: usize) {
        let mut right = vec![-1i32; self.sym_len.len()];
        let mut new_sym = vec![0u16; self.sym_len.len()];
        for c in chosen {
            let s = self.sym_len.len() as u16;
            self.pat.push([c.left, c.right]);
            self.sym_len.push(self.sym_len[c.left as usize] + self.sym_len[c.right as usize]);
            right[c.left as usize] = c.right as i32;
            new_sym[c.left as usize] = s;
        }
        let n = self.syms.len();
        let num_chunks = n.div_ceil(CHUNK_SYMS);
        let ends: Vec<AtomicUsize> = (0..num_chunks).map(|_| AtomicUsize::new(0)).collect();
        {
            // every chunk is compacted within its own borders: disjoint writes
            let syms = SharedSyms(self.syms.as_mut_ptr());
            let right = &right;
            let new_sym = &new_sym;
            chunks(n, workers, &|_, lo, hi| {
                let s = syms;
                let mut out = lo;
                let mut i = lo;
                while i < hi {
                    let c = unsafe { *s.0.add(i) };
                    let r = right[c as usize];
                    if r >= 0 && i + 1 < hi && unsafe { *s.0.add(i + 1) } == r as u16 {
                        unsafe { *s.0.add(out) = new_sym[c as usize] };
                        i += 2;
                    } else {
                        unsafe { *s.0.add(out) = c };
                        i += 1;
                    }
                    out += 1;
                }
                ends[lo / CHUNK_SYMS].store(out, Ordering::Relaxed);
            });
        }
        let mut out = 0;
        for (c, end) in ends.iter().enumerate() {
            let (lo, end) = (c * CHUNK_SYMS, end.load(Ordering::Relaxed));
            self.syms.copy_within(lo..end, out);
            out += end - lo;
        }
        self.syms.truncate(out);
    }

    /// Rewrites the whole stream with the dictionary of the sample: greedily
    /// the longest symbol whose expansion matches at the position, found by a
    /// trie over the expansions. Fixed chunks (no match crosses a border), so
    /// the result does not depend on the workers.
    fn tokenize(&self, values: &[u8], workers: usize) -> Vec<u16> {
        let num_syms = self.sym_len.len();
        let alphabet = self.alphabet;
        let mut expansions: Vec<Vec<u8>> = vec![Vec::new(); num_syms];
        for s in 0..num_syms {
            if s < alphabet {
                expansions[s] = vec![s as u8];
            } else {
                let (l, r) = (self.pat[s][0] as usize, self.pat[s][1] as usize);
                let mut x = expansions[l].clone(); // pairs refer to older symbols: already expanded
                x.extend_from_slice(&expansions[r]);
                expansions[s] = x;
            }
        }
        // the trie: children dense per node (alphabet entries), terminal =
        // the symbol whose expansion ends at the node (the lowest number)
        let mut children: Vec<i32> = vec![-1; alphabet];
        let mut terminal: Vec<i32> = vec![-1];
        for (s, x) in expansions.iter().enumerate() {
            let mut node = 0usize;
            for &v in x {
                let slot = node * alphabet + v as usize;
                if children[slot] < 0 {
                    children[slot] = terminal.len() as i32;
                    terminal.push(-1);
                    children.extend(std::iter::repeat_n(-1, alphabet));
                }
                node = children[slot] as usize;
            }
            if terminal[node] < 0 {
                terminal[node] = s as i32;
            }
        }
        let n = values.len();
        let num_chunks = n.div_ceil(CHUNK_SYMS);
        let parts: Vec<std::sync::Mutex<Vec<u16>>> = (0..num_chunks).map(|_| std::sync::Mutex::new(Vec::new())).collect();
        let (children, terminal) = (&children, &terminal);
        chunks(n, workers, &|_, lo, hi| {
            let mut out: Vec<u16> = Vec::with_capacity((hi - lo) / 8 + 16);
            let mut i = lo;
            while i < hi {
                let (mut node, mut best, mut best_len) = (0i32, values[i] as i32, 1usize);
                for j in i..hi {
                    node = children[node as usize * alphabet + values[j] as usize];
                    if node < 0 {
                        break;
                    }
                    let t = terminal[node as usize];
                    if t >= 0 {
                        best = t;
                        best_len = j - i + 1;
                    }
                }
                out.push(best as u16);
                i += best_len;
            }
            *parts[lo / CHUNK_SYMS].lock().unwrap() = out;
        });
        let mut tokens = Vec::new();
        for p in parts {
            tokens.extend_from_slice(&p.into_inner().unwrap());
        }
        tokens
    }

    /// Assigns the Huffman code and renumbers the symbols by code length.
    fn code(&self, tokens: &[u16]) -> Coded {
        let num_syms = self.sym_len.len();
        let mut freq = vec![0u64; num_syms];
        for &s in tokens {
            freq[s as usize] += 1;
        }
        let old_lengths = huffman_lengths(&freq, MAX_CODE_LEN);
        let mut order: Vec<usize> = (0..num_syms).collect();
        order.sort_by_key(|&s| if old_lengths[s] == 0 { 255u8 } else { old_lengths[s] });
        let mut new_id = vec![0u16; num_syms];
        for (i, &s) in order.iter().enumerate() {
            new_id[s] = i as u16;
        }
        let mut c = Coded { tokens: Vec::new(), table: vec![0; 3 * num_syms], sym_len: vec![0; num_syms], lengths: vec![0; num_syms], codes: Vec::new(), count: Vec::new(), min_len: 0, max_len: 0, passes: self.passes, symbols: num_syms };
        for (i, &s) in order.iter().enumerate() {
            c.lengths[i] = old_lengths[s];
            c.sym_len[i] = self.sym_len[s];
            let (l, r) = if s < self.alphabet { (s as u32, LEAF_MARK) } else { (new_id[self.pat[s][0] as usize] as u32, new_id[self.pat[s][1] as usize] as u32) };
            let w = l | r << 12;
            c.table[3 * i] = w as u8;
            c.table[3 * i + 1] = (w >> 8) as u8;
            c.table[3 * i + 2] = (w >> 16) as u8;
        }
        let (codes, count) = canonical_codes(&c.lengths);
        c.codes = codes;
        c.count = count;
        c.min_len = MAX_CODE_LEN;
        c.max_len = 0;
        for &l in &c.lengths {
            if l > 0 {
                c.min_len = c.min_len.min(l as usize);
                c.max_len = c.max_len.max(l as usize);
            }
        }
        if c.max_len == 0 {
            c.min_len = 0;
        }
        c.tokens = tokens.iter().map(|&s| new_id[s as usize]).collect();
        c
    }
}

/// A raw pointer into the symbol stream, shared by the chunk workers (each
/// writes only inside its own chunk).
#[derive(Clone, Copy)]
struct SharedSyms(*mut u16);
unsafe impl Send for SharedSyms {}
unsafe impl Sync for SharedSyms {}

struct Coded {
    tokens: Vec<u16>,
    table: Vec<u8>,
    sym_len: Vec<usize>,
    lengths: Vec<u8>,
    codes: Vec<u32>,
    count: Vec<usize>,
    min_len: usize,
    max_len: usize,
    passes: usize,
    symbols: usize,
}

impl Coded {
    /// Cuts the code into blocks and writes the container.
    fn container(&self, block_bytes: usize, mut idx_bits: usize) -> (Vec<u8>, Stats) {
        let mut block_bits = 0;
        while 1 << block_bits < block_bytes {
            block_bits += 1;
        }
        let block_bytes = 1usize << block_bits;
        let mut code: Vec<u8> = Vec::new();
        let mut sizes: Vec<u16> = Vec::new();
        let mut starts: Vec<usize> = Vec::new();
        let mut block: Vec<u8> = Vec::with_capacity(block_bytes);
        let mut acc = 0u64;
        let mut acc_bits = 0usize;
        let mut block_bits_used = 0usize;
        let mut block_positions = 0usize;
        let mut position = 0usize;
        let flush_block = |code: &mut Vec<u8>, sizes: &mut Vec<u16>, starts: &mut Vec<usize>, block: &mut Vec<u8>, acc: &mut u64, acc_bits: &mut usize, block_bits_used: &mut usize, block_positions: &mut usize, position: usize| {
            while *acc_bits > 0 {
                block.push((*acc >> 56) as u8);
                *acc <<= 8;
                *acc_bits = acc_bits.saturating_sub(8);
            }
            *acc = 0;
            while block.len() < block_bytes {
                block.push(0);
            }
            code.extend_from_slice(block);
            sizes.push((*block_positions - 1) as u16);
            starts.push(position - *block_positions);
            block.clear();
            *block_bits_used = 0;
            *block_positions = 0;
        };
        for &s in &self.tokens {
            let l = self.lengths[s as usize] as usize;
            let n = self.sym_len[s as usize];
            if block_positions > 0 && (block_bits_used + l > block_bytes * 8 || block_positions + n > POSITIONS_PER_BLOCK) {
                flush_block(&mut code, &mut sizes, &mut starts, &mut block, &mut acc, &mut acc_bits, &mut block_bits_used, &mut block_positions, position);
            }
            acc |= (self.codes[s as usize] as u64) << (64 - acc_bits - l);
            acc_bits += l;
            while acc_bits >= 8 {
                block.push((acc >> 56) as u8);
                acc <<= 8;
                acc_bits -= 8;
            }
            block_bits_used += l;
            block_positions += n;
            position += n;
        }
        if block_positions > 0 {
            flush_block(&mut code, &mut sizes, &mut starts, &mut block, &mut acc, &mut acc_bits, &mut block_bits_used, &mut block_positions, position);
        }
        let num_blocks = sizes.len();

        if idx_bits == 0 {
            idx_bits = 1;
            while (position >> idx_bits) * 16 > num_blocks && idx_bits < 40 {
                idx_bits += 1;
            }
        }
        let num_index = (position + (1 << idx_bits) - 1) >> idx_bits;
        let mut index = vec![0u8; 6 * num_index];
        let mut b = 0;
        for k in 0..num_index {
            let p = k << idx_bits;
            while b + 1 < num_blocks && starts[b + 1] <= p {
                b += 1;
            }
            index[6 * k..6 * k + 4].copy_from_slice(&(b as u32).to_le_bytes());
            index[6 * k + 4..6 * k + 6].copy_from_slice(&((p - starts[b]) as u16).to_le_bytes());
        }

        let h = if self.max_len > 0 { self.max_len - self.min_len + 1 } else { 0 };
        let mut out = Vec::with_capacity(HEADER_BYTES + 2 * h + self.table.len() + index.len() + 2 * num_blocks + code.len() + TAIL_BYTES);
        out.extend_from_slice(MAGIC);
        out.push(0);
        out.push(block_bits as u8);
        out.push(idx_bits as u8);
        out.push(self.max_len as u8);
        out.extend_from_slice(&(position as u64).to_le_bytes());
        out.extend_from_slice(&(self.symbols as u32).to_le_bytes());
        out.extend_from_slice(&(num_blocks as u32).to_le_bytes());
        out.push(self.min_len as u8);
        out.resize(HEADER_BYTES, 0);
        for l in self.min_len..=self.max_len {
            if h > 0 {
                out.extend_from_slice(&(self.count[l] as u16).to_le_bytes());
            }
        }
        out.extend_from_slice(&self.table);
        out.extend_from_slice(&index);
        for &sz in &sizes {
            out.extend_from_slice(&sz.to_le_bytes());
        }
        out.extend_from_slice(&code);
        out.resize(out.len() + TAIL_BYTES, 0);
        let st = Stats { positions: position, symbols: self.symbols, tokens: self.tokens.len(), blocks: num_blocks, block_bytes, index_bytes: index.len(), size_bytes: 2 * num_blocks, code_bytes: code.len(), bytes: out.len(), min_code_len: self.min_len, max_code_len: self.max_len, index_bits: idx_bits, passes: self.passes };
        (out, st)
    }
}

/// Code lengths for the frequencies (0 = unused), limited to max_len with
/// zlib's overflow repair.
fn huffman_lengths(freq: &[u64], max_len: usize) -> Vec<u8> {
    let mut leaves: Vec<usize> = (0..freq.len()).filter(|&s| freq[s] > 0).collect();
    let mut lengths = vec![0u8; freq.len()];
    match leaves.len() {
        0 => return lengths,
        1 => {
            lengths[leaves[0]] = 1;
            return lengths;
        }
        _ => {}
    }
    leaves.sort_by(|&a, &b| freq[a].cmp(&freq[b]).then(a.cmp(&b)));
    struct Node {
        weight: u64,
        parent: i32,
    }
    let mut nodes: Vec<Node> = leaves.iter().map(|&s| Node { weight: freq[s], parent: -1 }).collect();
    let (mut li, mut ii) = (0usize, leaves.len());
    let mut take_min = |nodes: &Vec<Node>| -> usize {
        if li < leaves.len() && (ii >= nodes.len() || nodes[li].weight <= nodes[ii].weight) {
            li += 1;
            li - 1
        } else {
            ii += 1;
            ii - 1
        }
    };
    for _ in 0..leaves.len() - 1 {
        let a = take_min(&nodes);
        let b = take_min(&nodes);
        let w = nodes[a].weight + nodes[b].weight;
        nodes.push(Node { weight: w, parent: -1 });
        let last = (nodes.len() - 1) as i32;
        nodes[a].parent = last;
        nodes[b].parent = last;
    }
    let mut depth = vec![0usize; nodes.len()];
    for i in (0..nodes.len() - 1).rev() {
        depth[i] = depth[nodes[i].parent as usize] + 1;
    }
    let mut bl_count = vec![0i64; max_len + 2];
    let mut overflow = 0i64;
    for &leaf_depth in depth.iter().take(leaves.len()) {
        let mut d = leaf_depth;
        if d > max_len {
            overflow += 1;
            d = max_len;
        }
        bl_count[d] += 1;
    }
    while overflow > 0 {
        let mut bits = max_len - 1;
        while bl_count[bits] == 0 {
            bits -= 1;
        }
        bl_count[bits] -= 1;
        bl_count[bits + 1] += 2;
        bl_count[max_len] -= 1;
        overflow -= 2;
    }
    let mut i = 0;
    for bits in (1..=max_len).rev() {
        for _ in 0..bl_count[bits] {
            lengths[leaves[i]] = bits as u8;
            i += 1;
        }
    }
    assert_eq!(i, leaves.len(), "pairs: huffman length assignment is inconsistent");
    let kraft: u64 = lengths.iter().filter(|&&l| l > 0).map(|&l| 1u64 << (max_len - l as usize)).sum();
    assert_eq!(kraft, 1u64 << max_len, "pairs: huffman code is not complete");
    lengths
}

/// Deflate-style canonical codes: codes of one length consecutive in symbol
/// order, longer codes continue numerically after the shorter ones.
fn canonical_codes(lengths: &[u8]) -> (Vec<u32>, Vec<usize>) {
    let mut count = vec![0usize; MAX_CODE_LEN + 2];
    for &l in lengths {
        count[l as usize] += 1;
    }
    count[0] = 0;
    let mut next = [0u32; MAX_CODE_LEN + 2];
    let mut code = 0u32;
    for bits in 1..=MAX_CODE_LEN {
        code = (code + count[bits - 1] as u32) << 1;
        next[bits] = code;
    }
    let mut codes = vec![0u32; lengths.len()];
    for (s, &l) in lengths.iter().enumerate() {
        if l > 0 {
            codes[s] = next[l as usize];
            next[l as usize] += 1;
        }
    }
    (codes, count)
}

// --- reading ---

/// Reads values from a packed container without unpacking it; the container
/// bytes are passed to every lookup (they may live in a file mapping).
pub struct Table {
    block_bits: u32,
    idx_bits: u32,
    min_len: usize,
    positions: usize,
    bytes: usize,
    table_off: usize,
    index_off: usize,
    sizes_off: usize,
    code_off: usize,
    sym_len: Vec<u16>,
    last: [u64; MAX_CODE_LEN + 2],
    first_code: [u32; MAX_CODE_LEN + 2],
    first_sym: [u32; MAX_CODE_LEN + 2],
    fast: Vec<u32>,
}

const FAST_BITS: u32 = 11;

fn le16(d: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([d[off], d[off + 1]])
}

fn le32(d: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([d[off], d[off + 1], d[off + 2], d[off + 3]])
}

impl Table {
    /// Parses a container (the data is not copied; pass the same bytes to value).
    pub fn open(data: &[u8]) -> Result<Table, String> {
        if data.len() < HEADER_BYTES || &data[..4] != MAGIC {
            return Err("pairs: not a pairs container".into());
        }
        let block_bits = data[5] as u32;
        let idx_bits = data[6] as u32;
        let max_len = data[7] as usize;
        let positions = u64::from_le_bytes(data[8..16].try_into().unwrap()) as usize;
        let num_syms = le32(data, 16) as usize;
        let num_blocks = le32(data, 20) as usize;
        let min_len = data[24] as usize;
        let num_index = (positions + (1 << idx_bits) - 1) >> idx_bits;
        let h = if max_len > 0 { max_len - min_len + 1 } else { 0 };
        let mut pos = HEADER_BYTES;
        let need = pos + 2 * h + 3 * num_syms + 6 * num_index + 2 * num_blocks + (num_blocks << block_bits) + TAIL_BYTES;
        if max_len > MAX_CODE_LEN || data.len() < need {
            return Err(format!("pairs: container truncated or corrupt ({} of {} bytes)", data.len(), need));
        }
        let mut count = [0usize; MAX_CODE_LEN + 2];
        for c in count.iter_mut().take(max_len + 1).skip(min_len) {
            if h > 0 {
                *c = le16(data, pos) as usize;
                pos += 2;
            }
        }
        let table_off = pos;
        pos += 3 * num_syms;
        let index_off = pos;
        pos += 6 * num_index;
        let sizes_off = pos;
        pos += 2 * num_blocks;
        let code_off = pos;
        let mut t = Table { block_bits, idx_bits, min_len, positions, bytes: need, table_off, index_off, sizes_off, code_off, sym_len: vec![0; num_syms], last: [0; MAX_CODE_LEN + 2], first_code: [0; MAX_CODE_LEN + 2], first_sym: [0; MAX_CODE_LEN + 2], fast: Vec::new() };
        for s in 0..num_syms {
            t.length_of(data, s as u32, 0)?;
        }
        let (mut code, mut sym) = (0u32, 0u32);
        for l in 1..=MAX_CODE_LEN {
            code = (code + count[l - 1] as u32) << 1;
            t.first_code[l] = code;
            t.first_sym[l] = sym;
            sym += count[l] as u32;
            let end = code as u64 + count[l] as u64;
            t.last[l] = if count[l] == 0 {
                0
            } else if end >= 1u64 << l {
                u64::MAX
            } else {
                (end << (64 - l)) - 1
            };
        }
        t.last[MAX_CODE_LEN + 1] = u64::MAX;
        if sym as usize > num_syms {
            return Err("pairs: more codes than symbols".into());
        }
        t.fast = vec![0u32; 1 << FAST_BITS];
        if max_len > 0 {
            for w in 0..1usize << FAST_BITS {
                let window = (w as u64) << (64 - FAST_BITS);
                let mut l = t.min_len;
                while window > t.last[l] {
                    l += 1;
                }
                if l as u32 <= FAST_BITS {
                    t.fast[w] = t.symbol_at(window, l) << 8 | l as u32;
                }
            }
        }
        Ok(t)
    }

    fn length_of(&mut self, data: &[u8], s: u32, depth: usize) -> Result<u16, String> {
        let n = self.sym_len.len();
        if s as usize >= n {
            return Err(format!("pairs: symbol {} beyond the table", s));
        }
        if self.sym_len[s as usize] != 0 {
            return Ok(self.sym_len[s as usize]);
        }
        if depth > n {
            return Err("pairs: the symbol table has a cycle".into());
        }
        let (l, r) = self.pair(data, s as usize);
        if r == LEAF_MARK {
            self.sym_len[s as usize] = 1;
            return Ok(1);
        }
        let a = self.length_of(data, l, depth + 1)?;
        let b = self.length_of(data, r, depth + 1)?;
        if a as usize + b as usize >= POSITIONS_PER_BLOCK {
            return Err(format!("pairs: symbol {} is too long", s));
        }
        self.sym_len[s as usize] = a + b;
        Ok(a + b)
    }

    #[inline]
    fn pair(&self, data: &[u8], s: usize) -> (u32, u32) {
        let p = &data[self.table_off + 3 * s..];
        let w = p[0] as u32 | (p[1] as u32) << 8 | (p[2] as u32) << 16;
        (w & 0xfff, w >> 12)
    }

    #[inline]
    fn symbol_at(&self, window: u64, l: usize) -> u32 {
        self.first_sym[l] + (window >> (64 - l)) as u32 - self.first_code[l]
    }

    /// The number of positions.
    pub fn len(&self) -> usize {
        self.positions
    }

    /// The size of the container.
    pub fn bytes(&self) -> usize {
        self.bytes
    }

    /// Decodes the value at a position.
    pub fn value(&self, data: &[u8], idx: usize) -> u8 {
        assert!(idx < self.positions, "pairs: position {} beyond {}", idx, self.positions);
        let entry = self.index_off + 6 * (idx >> self.idx_bits);
        let mut block = le32(data, entry) as usize;
        let mut lit = (idx & ((1 << self.idx_bits) - 1)) + le16(data, entry + 4) as usize;
        loop {
            let n = le16(data, self.sizes_off + 2 * block) as usize + 1;
            if lit < n {
                break;
            }
            lit -= n;
            block += 1;
        }
        let mut ptr = self.code_off + (block << self.block_bits);
        let mut window = u64::from_be_bytes(data[ptr..ptr + 8].try_into().unwrap());
        ptr += 8;
        let mut empty = 0usize;
        let mut sym;
        loop {
            let l;
            let f = self.fast[(window >> (64 - FAST_BITS)) as usize];
            if f != 0 {
                sym = f >> 8;
                l = (f & 0xff) as usize;
            } else {
                let mut ll = FAST_BITS as usize + 1;
                while window > self.last[ll] {
                    ll += 1;
                }
                l = ll;
                sym = self.symbol_at(window, l);
            }
            let n = self.sym_len[sym as usize] as usize;
            if lit < n {
                break;
            }
            lit -= n;
            window <<= l;
            empty += l;
            if empty >= 32 {
                empty -= 32;
                let next = if ptr + 4 <= data.len() {
                    let v = u32::from_be_bytes(data[ptr..ptr + 4].try_into().unwrap());
                    ptr += 4;
                    v
                } else {
                    0
                };
                window |= (next as u64) << empty;
            }
        }
        loop {
            let (l, r) = self.pair(data, sym as usize);
            if r == LEAF_MARK {
                return l as u8;
            }
            let n = self.sym_len[l as usize] as usize;
            if lit < n {
                sym = l;
            } else {
                lit -= n;
                sym = r;
            }
        }
    }
}
