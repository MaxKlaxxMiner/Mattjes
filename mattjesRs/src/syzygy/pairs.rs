//! The compressed value stream of a Syzygy table (Fathom's setup_pairs and
//! decompress_pairs, see the Go package `syzygy/pairs.go`): values are
//! run-length "pairs" encoded as a canonical Huffman code over symbols,
//! where a symbol expands recursively into two others down to single
//! values; blocks of 2^block_size bytes hold the code, an index table maps
//! every 2^idx_bits-th position to its block and offset, a size table gives
//! the positions per block. The tables live in the mapped file; this struct
//! holds offsets into it.

#[derive(Clone, Default)]
pub struct PairsData {
    /// Offsets into the file: 6 bytes per index entry (block u32, offset u16).
    pub index_table: usize,
    /// u16 per block: positions in the block minus one.
    pub size_table: usize,
    /// The code blocks (to the end of the file: the decoder reads ahead).
    pub data: usize,
    /// u16 per code length (from min_len): the first symbol of that length.
    pub offset: usize,
    /// 3 bytes per symbol: two sub-symbols, or the value.
    pub sym_pat: usize,
    /// Per symbol: the positions it expands to minus one.
    pub sym_len: Vec<u8>,
    /// Per code length (from min_len): the first code, left-aligned in 64 bits.
    pub base: Vec<u64>,
    pub block_size: u8,
    pub idx_bits: u8,
    pub min_len: u8,
    /// The whole table has one value (idx_bits 0).
    pub const_value: [u8; 2],
    /// Positions in the table.
    pub size: u64,
    /// Code blocks in the file (the size table may list padding blocks).
    pub real_blocks: usize,
}

fn le16(d: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([d[at], d[at + 1]])
}
fn le32(d: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([d[at], d[at + 1], d[at + 2], d[at + 3]])
}
fn be32(d: &[u8], at: usize) -> u32 {
    let mut v = 0u32;
    for i in 0..4 {
        // the very end of the file: pad, the bits are never used
        let b = if at + i < d.len() { d[at + i] } else { 0 };
        v = v << 8 | b as u32;
    }
    v
}
fn be64(d: &[u8], at: usize) -> u64 {
    (be32(d, at) as u64) << 32 | be32(d, at + 4) as u64
}

impl PairsData {
    fn calc_sym_len(&mut self, d: &[u8], s: usize, tmp: &mut [bool]) {
        let w = self.sym_pat + 3 * s;
        let s2 = ((d[w + 2] as usize) << 4) | (d[w + 1] as usize >> 4);
        if s2 == 0x0fff {
            self.sym_len[s] = 0;
        } else {
            let s1 = ((d[w + 1] as usize & 0xf) << 8) | d[w] as usize;
            if !tmp[s1] {
                self.calc_sym_len(d, s1, tmp);
            }
            if !tmp[s2] {
                self.calc_sym_len(d, s2, tmp);
            }
            self.sym_len[s] = self.sym_len[s1] + self.sym_len[s2] + 1;
        }
        tmp[s] = true;
    }

    /// Reads the pairs header at d[pos..] for a table of tb_size positions;
    /// returns the pairs data, the position after the header and the byte
    /// sizes of the index table, the size table and the code blocks.
    pub fn setup(d: &[u8], pos: usize, tb_size: u64) -> (PairsData, usize, [usize; 3]) {
        let mut p = PairsData { size: tb_size, ..Default::default() };
        let flags = d[pos];
        if flags & 0x80 != 0 {
            p.idx_bits = 0;
            p.const_value[0] = d[pos + 1];
            return (p, pos + 2, [0, 0, 0]);
        }
        p.block_size = d[pos + 1];
        p.idx_bits = d[pos + 2];
        let real_num_blocks = le32(d, pos + 4);
        p.real_blocks = real_num_blocks as usize;
        let num_blocks = real_num_blocks + d[pos + 3] as u32;
        let max_len = d[pos + 8] as usize;
        let min_len = d[pos + 9] as usize;
        let h = max_len - min_len + 1;
        let num_syms = le16(d, pos + 10 + 2 * h) as usize;
        p.offset = pos + 10;
        p.sym_len = vec![0u8; num_syms];
        p.sym_pat = pos + 12 + 2 * h;
        p.min_len = min_len as u8;
        let next = pos + 12 + 2 * h + 3 * num_syms + (num_syms & 1);
        let num_indices = (tb_size + (1u64 << p.idx_bits) - 1) >> p.idx_bits;
        let sizes = [6 * num_indices as usize, 2 * num_blocks as usize, (real_num_blocks as usize) << p.block_size];
        let mut tmp = vec![false; num_syms];
        for s in 0..num_syms {
            if !tmp[s] {
                p.calc_sym_len(d, s, &mut tmp);
            }
        }
        p.base = vec![0u64; h];
        for i in (0..h.saturating_sub(1)).rev() {
            p.base[i] = (p.base[i + 1] + le16(d, p.offset + 2 * i) as u64 - le16(d, p.offset + 2 * i + 2) as u64) / 2;
        }
        for i in 0..h {
            p.base[i] <<= 64 - (min_len + i) as u32;
        }
        (p, next, sizes)
    }

    /// Returns the value byte of the position idx.
    pub fn decompress(&self, d: &[u8], idx: u64) -> u8 {
        if self.idx_bits == 0 {
            return self.const_value[0];
        }
        assert!(idx < self.size, "syzygy: index {} beyond the table size {} (an encoding bug)", idx, self.size);
        let main_idx = (idx >> self.idx_bits) as usize;
        let mut lit_idx = (idx & ((1u64 << self.idx_bits) - 1)) as i64 - (1i64 << (self.idx_bits - 1));
        let mut block = le32(d, self.index_table + 6 * main_idx) as usize;
        lit_idx += le16(d, self.index_table + 6 * main_idx + 4) as i64;
        let size_at = |b: usize| le16(d, self.size_table + 2 * b) as i64;
        if lit_idx < 0 {
            while lit_idx < 0 {
                block -= 1;
                lit_idx += size_at(block) + 1;
            }
        } else {
            while lit_idx > size_at(block) {
                lit_idx -= size_at(block) + 1;
                block += 1;
            }
        }
        assert!(block < self.real_blocks, "syzygy: block {} of {} (an encoding bug)", block, self.real_blocks);
        let mut ptr = self.data + (block << self.block_size);
        let m = self.min_len as usize;
        let mut code = be64(d, ptr);
        ptr += 8;
        let mut bit_cnt = 0u32; // empty bits in code
        let mut sym: usize;
        loop {
            let mut l = m;
            while code < self.base[l - m] {
                l += 1;
            }
            sym = le16(d, self.offset + 2 * (l - m)) as usize;
            sym += ((code - self.base[l - m]) >> (64 - l as u32)) as usize;
            if lit_idx < self.sym_len[sym] as i64 + 1 {
                break;
            }
            lit_idx -= self.sym_len[sym] as i64 + 1;
            code <<= l as u32;
            bit_cnt += l as u32;
            if bit_cnt >= 32 {
                bit_cnt -= 32;
                let tmp = be32(d, ptr);
                ptr += 4;
                code |= (tmp as u64) << bit_cnt;
            }
        }
        while self.sym_len[sym] != 0 {
            let w = self.sym_pat + 3 * sym;
            let s1 = ((d[w + 1] as usize & 0xf) << 8) | d[w] as usize;
            if lit_idx < self.sym_len[s1] as i64 + 1 {
                sym = s1;
            } else {
                lit_idx -= self.sym_len[s1] as i64 + 1;
                sym = ((d[w + 2] as usize) << 4) | (d[w + 1] as usize >> 4);
            }
        }
        d[self.sym_pat + 3 * sym]
    }
}
