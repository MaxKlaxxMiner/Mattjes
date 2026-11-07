//! Own WDL and DTM files packed from the exact files of the raw sweep
//! (2026-11-07, card MT-8): variants of the packer measured for size,
//! checked against the exact tables and timed against Syzygy's WDL files.
//!
//!   egtb-pack <names> <raw dir> <out dir> [key=value ...] [--workers N]
//!   egtb-speed <names> <own dir> <syzygy dir> [samples] [--workers N]
//!
//! names: "3", "4", "5", "6" (every material with that many pieces, several
//! comma separated) or material names separated by ";". The raw dir holds
//! 4-all.bin and dtm/<n>-<Name>.bin.raw (the sweep's cache folder).

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::bitboard::Board;
use crate::chess::{Piece, Pos};
use crate::egtb::{self, Material, Set, Value};
use crate::pairs;
use crate::syzygy::{self, write::BuildOptions, Tables};

/// What one packing run writes.
#[derive(Clone)]
pub struct Spec {
    /// "wdl" or "dtm".
    pub mode: String,
    /// DTM: moves per bucket.
    pub step: u32,
    /// DTM: wins and losses as separate symbols (else the WDL file tells).
    pub signed: bool,
    /// DTM: draws stored as a symbol instead of don't care.
    pub draws_stored: bool,
    pub opt: pairs::Options,
    pub bo: BuildOptions,
    /// Random positions checked against the exact table after writing.
    pub verify: usize,
    /// syzygy::write::FILL_MODE.
    pub fill: u8,
    /// syzygy::write::PROMOTION_BOUND (the promotion targets are loaded too).
    pub promo: bool,
    /// DTM: keep only the side to move that packs smaller.
    pub one_side: bool,
    /// DTM: the folder of our WDL files, for checking the distance bounds as
    /// the reader derives them (`probe_dtm_bounds`).
    pub wdl_dir: String,
}

impl Spec {
    /// Reads key=value settings (unknown keys are reported and ignored).
    pub fn parse(args: &[String], workers: usize) -> Spec {
        let mut s = Spec { mode: "wdl".into(), step: 8, signed: false, draws_stored: false, opt: pairs::Options { workers, ..Default::default() }, bo: BuildOptions::default(), verify: 200_000, fill: 0, promo: false, one_side: false, wdl_dir: String::new() };
        for a in args {
            let Some((k, v)) = a.split_once('=') else { continue };
            let n = || v.parse::<usize>().unwrap_or(0);
            match k {
                "mode" => s.mode = v.to_string(),
                "step" => s.step = n() as u32,
                "signed" => s.signed = n() != 0,
                "draws" => s.draws_stored = v == "stored",
                "block" => s.opt.block_bytes = n(),
                "indexbits" => s.opt.index_bits = n(),
                "symbols" => s.opt.max_symbols = n(),
                "symlen" => s.opt.max_symbol_len = n(),
                "mincount" => s.opt.min_count = n(),
                "pairs" => s.opt.pairs_per_pass = n(),
                "dictsample" => s.bo.dict_sample = n(),
                "dictabove" => s.bo.dict_above = n(),
                "target" => s.bo.sample_target = n(),
                "finalists" => s.bo.finalists = n(),
                "window" => s.bo.window = n(),
                "samplesymbols" => s.bo.sample_symbols = n(),
                "verify" => s.verify = n(),
                "orders" => s.bo.orders = parse_orders(v),
                "oneside" => s.one_side = n() != 0,
                "wdldir" => s.wdl_dir = v.to_string(),
                "promo" => {
                    s.promo = n() != 0;
                    syzygy::write::PROMOTION_BOUND.store(s.promo, std::sync::atomic::Ordering::Relaxed);
                }
                "fill" => {
                    s.fill = n() as u8;
                    syzygy::write::FILL_MODE.store(s.fill, std::sync::atomic::Ordering::Relaxed);
                }
                _ => println!("egtb-pack: unknown setting {}", a),
            }
        }
        s
    }

    fn describe(&self) -> String {
        let o = &self.opt;
        let b = &self.bo;
        let mut d = format!("mode={} fill={} promo={}", self.mode, self.fill, self.promo as u8);
        if self.mode == "dtm" {
            d += &format!(" step={} signed={} draws={} oneside={} wdldir={}", self.step, self.signed as u8, if self.draws_stored { "stored" } else { "free" }, self.one_side as u8, self.wdl_dir);
        }
        d + &format!(
            " block={} indexbits={} symbols={} symlen={} mincount={} pairs={} dictsample={} dictabove={} target={} finalists={} window={} samplesymbols={}",
            o.block_bytes, o.index_bits, o.max_symbols, o.max_symbol_len, o.min_count, o.pairs_per_pass, b.dict_sample, b.dict_above, b.sample_target, b.finalists, b.window, b.sample_symbols
        )
    }

    fn extension(&self) -> &'static str {
        if self.mode == "dtm" {
            "dtm"
        } else {
            "wdl"
        }
    }
}

/// Reads given piece orders: sides separated by '|', orders of a side by
/// ',', each as letters (KQRBNP white, lower case black) and "/order" or
/// "/order/order2", as `Order` prints them.
fn parse_orders(v: &str) -> Vec<Vec<syzygy::write::Order>> {
    v.split('|')
        .map(|side| {
            side.split(',')
                .filter_map(|o| {
                    let mut parts = o.split('/');
                    let letters = parts.next()?;
                    let pieces = letters
                        .chars()
                        .map(|c| {
                            let kind = match c.to_ascii_uppercase() {
                                'P' => 1,
                                'N' => 2,
                                'B' => 3,
                                'R' => 4,
                                'Q' => 5,
                                _ => 6,
                            };
                            kind | if c.is_ascii_lowercase() { 8 } else { 0 }
                        })
                        .collect();
                    let order = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0);
                    let order2 = parts.next().and_then(|x| x.parse().ok()).unwrap_or(0x0f);
                    Some(syzygy::write::Order { pieces, order, order2 })
                })
                .collect()
        })
        .collect()
}

/// The materials of a names argument, in dependency order per piece count.
pub fn materials_of(names: &str) -> Vec<Material> {
    let mut out = Vec::new();
    for part in names.split([',', ';']) {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        if let Ok(n) = part.parse::<usize>() {
            if n <= 4 {
                out.extend(Material::all().into_iter().filter(|m| m.pieces() == n));
            } else {
                out.extend(Material::all_with(n));
            }
        } else {
            match Material::parse(part) {
                Ok(m) => out.push(m),
                Err(e) => println!("{}: {}", part, e),
            }
        }
    }
    out
}

fn log_line(dir: &Path, line: &str) {
    println!("{}", line);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("pack.log")) {
        let _ = writeln!(f, "{}", line);
    }
}

/// The pieces of a material in the order `Board::from_pieces` takes them.
fn pieces_of(m: &Material) -> Vec<Piece> {
    let mut pieces = vec![Piece::WHITE_KING, Piece::BLACK_KING];
    pieces.extend(m.white.iter().map(|p| Piece(Piece::WHITE.0 | p.0)));
    pieces.extend(m.black.iter().map(|p| Piece(Piece::BLACK.0 | p.0)));
    pieces
}

/// A small deterministic generator (xorshift).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

/// Random legal positions of a material (side to move random, no pawn on
/// the first or last rank, the side not to move not in check).
fn random_positions(m: &Material, count: usize, seed: u64) -> Vec<Board> {
    let pieces = pieces_of(m);
    let mut squares = vec![Pos(0); pieces.len()];
    let mut rng = Rng(seed | 1);
    let mut out = Vec::with_capacity(count);
    while out.len() < count {
        let mut used = 0u64;
        for (i, p) in pieces.iter().enumerate() {
            loop {
                let sq = rng.below(64) as usize;
                if used & (1 << sq) != 0 || ((p.0 & Piece::TYPE_MASK.0) == Piece::PAWN.0 && !(8..56).contains(&sq)) {
                    continue;
                }
                used |= 1 << sq;
                squares[i] = Pos(sq as i8);
                break;
            }
        }
        let b = Board::from_pieces(rng.below(2) == 0, &squares, &pieces);
        if !b.opponent_in_check() {
            out.push(b);
        }
    }
    out
}

fn class_of(v: Value) -> i32 {
    if v.is_win() {
        1
    } else if v.is_loss() {
        -1
    } else {
        0
    }
}

fn class_of_wdl(v: i32) -> i32 {
    if v >= syzygy::CURSED_WIN {
        1
    } else if v <= syzygy::BLESSED_LOSS {
        -1
    } else {
        0
    }
}

/// Packs the WDL or DTM files of the named materials from their exact files
/// into out_dir (materials whose file exists are skipped, so a killed run
/// resumes), checks random positions against the exact table and logs one
/// line per material to out_dir/pack.log.
pub fn egtb_pack(names: &str, workers: usize, raw_dir: &str, out_dir: &str, spec: &Spec) {
    let out = PathBuf::from(out_dir);
    if let Err(e) = std::fs::create_dir_all(&out) {
        println!("egtb-pack: {}: {}", out.display(), e);
        return;
    }
    let mut quiet = |_: &str| {};
    let mut set = Set::load_or_generate(&Path::new(raw_dir).join(egtb::BASE_FILE_NAME), workers, false, &mut quiet);
    set.cache_dir = PathBuf::from(raw_dir);
    set.pack_deps = true; // capture dependencies held packed: only looked up for the capture bound
    // the four-piece base alone: the reader's exact answers below the files (DTM bounds check)
    let base_only = Set::load_or_generate(&Path::new(raw_dir).join(egtb::BASE_FILE_NAME), workers, false, &mut quiet);
    log_line(&out, &format!("# egtb-pack {} workers={} raw={} {}", names, workers, raw_dir, spec.describe()));
    let start = Instant::now();
    let (mut total, mut total_syzygy, mut done) = (0u64, 0u64, 0usize);
    for m in materials_of(names) {
        let path = out.join(format!("{}-{}.{}", m.pieces(), m.name(), spec.extension()));
        if path.exists() {
            continue;
        }
        let t_start = Instant::now();
        // the capture dependencies (fewer pieces): the base, or exact files packed in memory
        let mut missing = false;
        for d in m.dependencies() {
            if d.pieces() >= m.pieces() {
                continue; // a promotion without capture keeps the piece count: our WDL files written before answer it
            }
            let di = set.add_material(d);
            if !set.tables[di].is_available() && !set.load_raw(di, &mut quiet) {
                println!("{}: dependency {} has no exact file", m.name(), set.tables[di].mat.name());
                missing = true;
            }
        }
        if missing {
            continue;
        }
        let ti = match set.find(&m.name()) {
            Some(ti) if m.pieces() <= 4 => ti,
            _ => {
                let ti = set.add_material(m.clone());
                let raw = set.raw_path(&set.tables[ti]);
                if let Err(e) = set.load_raw_table_checked(ti, &raw, true) {
                    println!("{}: {}", m.name(), e);
                    continue;
                }
                ti
            }
        };
        let loaded = t_start.elapsed().as_secs_f64();
        let b_start = Instant::now();
        let t = &set.tables[ti];
        // the promotion targets: our WDL files of this folder (written earlier in the order)
        let mut own = Tables::new();
        if spec.promo {
            let _ = own.add_own(&out);
        }
        let src = if spec.mode == "dtm" {
            syzygy::write::Source::new_dtm(&set, t, spec.step, spec.signed, !spec.draws_stored, workers)
        } else {
            syzygy::write::Source::new_with(&set, t, if spec.promo { Some(&own) } else { None }, workers)
        };
        let derived = b_start.elapsed().as_secs_f64();
        let mut built = syzygy::write::build(&src, spec.opt, spec.bo.clone(), workers, None);
        let (legal, decided, alphabet) = (src.legal, src.decided, src.alphabet);
        let orders = built.streams.iter().map(|s| s.order.to_string()).collect::<Vec<_>>().join(",");
        // the bytes per side to move (a one-sided file would keep the smaller)
        let per_side: Vec<usize> = (0..built.sides).map(|s| built.streams[s * built.files..(s + 1) * built.files].iter().map(|st| st.packed.len()).sum()).collect();
        let mut orders = format!("{} side_bytes={}", orders, per_side.iter().map(|b| b.to_string()).collect::<Vec<_>>().join("+"));
        if spec.one_side {
            if let Some(s) = built.keep_smaller_side() {
                orders += &format!(" stored_side={}", s);
            }
        }
        drop(src);
        if let Err(e) = built.write(&path) {
            println!("{}: {}", m.name(), e);
            continue;
        }
        let build_s = b_start.elapsed().as_secs_f64();
        let bytes = std::fs::metadata(&path).map(|md| md.len()).unwrap_or(0);
        let syz = syzygy_size(&m);
        let check = verify_file(&set, &m, &out, spec, ti, &base_only);
        let ratio = if spec.mode == "wdl" && syz > 0 { format!(" ratio={:.3}", bytes as f64 / syz as f64) } else { String::new() };
        log_line(
            &out,
            &format!(
                "{} pieces={} legal={} decided={:.1}% alphabet={} bytes={} syzygy={}{} candidates={} load_s={:.1} derive_s={:.1} build_s={:.1} {} orders={}",
                m.name(),
                m.pieces(),
                legal,
                100.0 * decided as f64 / legal.max(1) as f64,
                alphabet,
                bytes,
                syz,
                ratio,
                built.candidates,
                loaded,
                derived,
                build_s,
                check,
                orders
            ),
        );
        total += bytes;
        total_syzygy += syz;
        done += 1;
        if m.pieces() > 4 {
            set.release(ti); // the capture dependencies stay for the next material
        }
    }
    log_line(&out, &format!("# done: {} files, {} bytes (syzygy wdl {} bytes), {:.1} s", done, total, total_syzygy, start.elapsed().as_secs_f64()));
}

/// The size of Syzygy's WDL file of a material (0 without one).
fn syzygy_size(m: &Material) -> u64 {
    let dir = std::env::var("MATTJES_SYZYGY").unwrap_or_else(|_| "/home/admin/chess/table".to_string());
    let name = m.name();
    let file = format!("{}{}", &name[..1], name[1..].replacen('K', "vK", 1));
    std::fs::metadata(format!("{}/{}.rtbw", dir, file)).map(|md| md.len()).unwrap_or(0)
}

/// Probes the written file through the reader on random positions against
/// the exact table (WDL: the class with captures resolved; DTM: the bucket of
/// every win and loss).
fn verify_file(set: &Set, m: &Material, dir: &Path, spec: &Spec, ti: usize, base_only: &Set) -> String {
    if spec.verify == 0 {
        return "verify=skipped".into();
    }
    let mut tb = Tables::new();
    let registered = if spec.mode == "dtm" { tb.add_dtm(dir) } else { tb.add_own(dir) };
    if registered.unwrap_or(0) == 0 {
        return "verify=NO FILE".into();
    }
    if spec.mode == "dtm" && !spec.wdl_dir.is_empty() {
        return verify_dtm_bounds(set, m, &mut tb, spec, base_only);
    }
    let t = &set.tables[ti];
    let top = if t.is_wide() { u32::MAX } else { 127 };
    let positions = random_positions(m, spec.verify, 0x9E3779B97F4A7C15 ^ m.name().len() as u64);
    let (mut bad, mut checked) = (0usize, 0usize);
    let start = Instant::now();
    for b in &positions {
        let Some(exact) = set.lookup(b) else { continue };
        if spec.mode == "dtm" {
            if exact == Value::DRAW && !spec.draws_stored {
                continue;
            }
            checked += 1;
            let Some((bucket, _)) = tb.probe_dtm(b) else {
                bad += 1;
                continue;
            };
            let q = exact.quantize_to(spec.step, top);
            let want = q.moves().div_ceil(spec.step) as usize;
            if !spec.signed && bucket != want {
                // (signed files are only checked for a value: their loss symbols are offset)
                bad += 1;
                if bad <= 3 {
                    println!("  MISMATCH {}: exact {}, bucket {} want {}", b.fen(), exact, bucket, want);
                }
            }
        } else {
            checked += 1;
            match tb.probe_wdl_with(b, Some(set)) {
                Some(v) if class_of_wdl(v) == class_of(exact) => {}
                other => {
                    bad += 1;
                    if bad <= 3 {
                        println!("  MISMATCH {}: exact {}, file {:?}", b.fen(), exact, other);
                    }
                }
            }
        }
    }
    let status = if bad == 0 { "ok".to_string() } else { format!("{}_MISMATCHES", bad) };
    format!("verify={}/{}:{} ({:.1} s)", checked, positions.len(), status, start.elapsed().as_secs_f64())
}

/// Packs Syzygy's own decoded WDL streams with our packer (the five values,
/// and mapped to our three: cursed results count as results) and prints the
/// sizes next to the bytes the stream takes in the Syzygy file: the same
/// data and fill, so the difference is the packer; against our own file of
/// the material it shows what our streams (order, fill) cost.
pub fn egtb_syzstream(names: &str, syzygy_dir: &str, own_dir: &str, opt: pairs::Options) {
    let syz = match Tables::open(Path::new(syzygy_dir)) {
        Ok(t) => t,
        Err(e) => {
            println!("syzygy: {}", e);
            return;
        }
    };
    let (mut s5, mut s3, mut sz, mut so) = (0usize, 0usize, 0usize, 0u64);
    for m in materials_of(names) {
        let name = m.name();
        let file = format!("{}{}", &name[..1], name[1..].replacen('K', "vK", 1));
        let Some(streams) = syz.streams(&file) else {
            println!("{}: no Syzygy table", name);
            continue;
        };
        let (mut m5, mut m3, mut mz) = (0usize, 0usize, 0usize);
        for (i, (values, bytes, pieces)) in streams.iter().enumerate() {
            let p5 = pairs::pack(values, 5, opt).len();
            let three: Vec<u8> = values.iter().map(|&v| [0u8, 0, 1, 2, 2][v as usize]).collect();
            let p3 = pairs::pack(&three, 3, opt).len();
            println!("  {} stream {} pieces {:?}: {} positions, syzygy {} bytes, ours 5 values {}, ours 3 values {}", name, i, pieces, values.len(), bytes, p5, p3);
            m5 += p5;
            m3 += p3;
            mz += bytes;
        }
        let own = std::fs::metadata(Path::new(own_dir).join(format!("{}-{}.wdl", m.pieces(), name))).map(|md| md.len()).unwrap_or(0);
        println!("{} syzygy_streams={} ours5={} ({:.3}) ours3={} ({:.3}) own_file={} ({:.3})", name, mz, m5, m5 as f64 / mz as f64, m3, m3 as f64 / mz as f64, own, own as f64 / mz as f64);
        s5 += m5;
        s3 += m3;
        sz += mz;
        so += own;
    }
    println!("# total syzygy_streams={} ours5={} ours3={} own_files={}", sz, s5, s3, so);
}

/// Compares our WDL stream of one side (in a given order, which must be
/// Syzygy's for the comparison to mean anything) position by position with
/// Syzygy's decoded stream of a pawnless material: where they differ, and
/// whether a stream with our fixed symbols and Syzygy's choices at our free
/// positions packs like Syzygy's.
pub fn egtb_cmpstream(name: &str, raw_dir: &str, syzygy_dir: &str, order: &str, side: usize, workers: usize) {
    let m = match Material::parse(name) {
        Ok(m) => m,
        Err(e) => {
            println!("{}: {}", name, e);
            return;
        }
    };
    let mut quiet = |_: &str| {};
    let mut set = Set::load_or_generate(&Path::new(raw_dir).join(egtb::BASE_FILE_NAME), workers, false, &mut quiet);
    set.cache_dir = PathBuf::from(raw_dir);
    set.pack_deps = true;
    for d in m.dependencies() {
        if d.pieces() < m.pieces() {
            let di = set.add_material(d);
            if !set.tables[di].is_available() {
                set.load_raw(di, &mut quiet);
            }
        }
    }
    let ti = match set.find(&m.name()) {
        Some(ti) if m.pieces() <= 4 => ti,
        _ => {
            let ti = set.add_material(m.clone());
            let raw = set.raw_path(&set.tables[ti]);
            set.load_raw_table_checked(ti, &raw, true).expect("raw table");
            ti
        }
    };
    let src = syzygy::write::Source::new(&set, &set.tables[ti], workers);
    let o = &parse_orders(order)[0][0];
    let raw = &syzygy::write::side_streams(&src, side, o, false, workers)[0];
    let filled = &syzygy::write::side_streams(&src, side, o, true, workers)[0];
    let syz = Tables::open(Path::new(syzygy_dir)).expect("syzygy");
    let file = format!("{}{}", &name[..1], name[1..].replacen('K', "vK", 1));
    let streams = syz.streams(&file).expect("syzygy table");
    let (theirs5, _, pieces) = &streams[side.min(streams.len() - 1)];
    let theirs: Vec<u8> = theirs5.iter().map(|&v| [0u8, 0, 1, 2, 2][v as usize]).collect();
    println!("{} side {} order {} (syzygy pieces {:?}): {} positions ours, {} theirs", name, side, order, pieces, raw.len(), theirs.len());
    if raw.len() != theirs.len() {
        return;
    }
    let (mut plain_same, mut plain_diff, mut dc, mut dc_same, mut bd, mut bd_same, mut bd_violate) = (0usize, 0usize, 0usize, 0usize, 0usize, 0usize, 0usize);
    let mut hybrid = filled.clone();
    for i in 0..raw.len() {
        let (r, t) = (raw[i], theirs[i]);
        if r == syzygy::write::DONT_CARE {
            dc += 1;
            dc_same += (filled[i] == t) as usize;
            hybrid[i] = t;
        } else if r & 0x80 != 0 {
            bd += 1;
            bd_same += (filled[i] == t) as usize;
            if t <= r & 0x7f {
                hybrid[i] = t;
            } else {
                bd_violate += 1;
            }
        } else if r == t {
            plain_same += 1;
        } else {
            plain_diff += 1;
            if plain_diff <= 5 {
                let mut sq = vec![Pos(0); m.pieces()];
                let _ = &mut sq;
                println!("  plain differs at stream index {}: ours {} theirs {} (five-valued {})", i, r, t, theirs5[i]);
            }
        }
    }
    println!("plain: {} same, {} differ; don't care: {} ({} filled as Syzygy); bounded: {} ({} filled as Syzygy, {} where Syzygy exceeds our bound)", plain_same, plain_diff, dc, dc_same, bd, bd_same, bd_violate);
    // the free runs where a choice differs: left and right plain neighbour, the run's cap, its length, ours and theirs
    let mut hist: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let free = |s: u8| s == syzygy::write::DONT_CARE || s & 0x80 != 0;
    let mut i = 0;
    while i < raw.len() {
        if !free(raw[i]) {
            i += 1;
            continue;
        }
        let mut j = i;
        let mut cap = 2u8;
        while j < raw.len() && free(raw[j]) {
            if raw[j] != syzygy::write::DONT_CARE {
                cap = cap.min(raw[j] & 0x7f);
            }
            j += 1;
        }
        if (i..j).any(|k| filled[k] != theirs[k]) {
            let l = if i > 0 { raw[i - 1].to_string() } else { "-".into() };
            let r = if j < raw.len() { raw[j].to_string() } else { "-".into() };
            let ours: String = (i..j.min(i + 8)).map(|k| filled[k].to_string()).collect();
            let th: String = (i..j.min(i + 8)).map(|k| theirs[k].to_string()).collect();
            let caps: String = (i..j.min(i + 8)).map(|k| if raw[k] == syzygy::write::DONT_CARE { "*".to_string() } else { (raw[k] & 0x7f).to_string() }).collect();
            let len = if j - i > 8 { ">8".to_string() } else { (j - i).to_string() };
            *hist.entry(format!("L={} R={} cap={} len={} free={} ours={} theirs={}", l, r, cap, len, caps, ours, th)).or_default() += 1;
        }
        i = j;
    }
    let mut h: Vec<_> = hist.into_iter().collect();
    h.sort_by_key(|e| std::cmp::Reverse(e.1));
    for (k, n) in h.iter().take(25) {
        println!("  {:>7}  {}", n, k);
    }
    let opt = pairs::Options { workers, ..Default::default() };
    let pack = |v: &[u8]| pairs::pack(v, 3, opt).len();
    println!("packed: ours {} bytes, theirs {} bytes, ours with Syzygy's choices at free positions {} bytes", pack(filled), pack(&theirs), pack(&hybrid));
    for mode in [0u8, 1, 3, 4] {
        let mut v = raw.clone();
        syzygy::write::fill_mode(&mut v, mode);
        let same = (0..v.len()).filter(|&k| free(raw[k]) && v[k] == theirs[k]).count();
        println!("fill mode {}: {} bytes, {} free positions as Syzygy", mode, pack(&v), same);
    }
}

/// Checks the distance bounds the reader derives (`probe_dtm_bounds`: our
/// WDL files of spec.wdl_dir, the DTM files just written, the four-piece base
/// below) on random positions of both sides against the exact table: the
/// verdict must match and the exact distance lie in the bounds. Reports the
/// mean width and the time per probe.
fn verify_dtm_bounds(set: &Set, m: &Material, tb: &mut Tables, spec: &Spec, base_only: &Set) -> String {
    if tb.add_own(Path::new(&spec.wdl_dir)).unwrap_or(0) == 0 {
        return "verify=NO WDL FILES".into();
    }
    let positions = random_positions(m, spec.verify, 0x9E3779B97F4A7C15 ^ m.name().len() as u64);
    let (mut bad, mut checked, mut unknown, mut width, mut decided) = (0usize, 0usize, 0usize, 0u64, 0usize);
    let start = Instant::now();
    for b in &positions {
        let Some(exact) = set.lookup(b) else { continue };
        checked += 1;
        let Some((wdl, lo, hi)) = tb.probe_dtm_bounds(b, Some(base_only)) else {
            unknown += 1;
            continue;
        };
        let p = exact.plies();
        if wdl != class_of(exact) || (wdl != 0 && (p < lo || p > hi)) {
            bad += 1;
            if bad <= 3 {
                println!("  MISMATCH {}: exact {}, probe {} [{}, {}]", b.fen(), exact, wdl, lo, hi);
            }
        }
        if wdl != 0 {
            decided += 1;
            width += (hi - lo) as u64;
        }
    }
    let el = start.elapsed().as_secs_f64();
    let status = if bad == 0 && unknown == 0 { "ok".to_string() } else { format!("{}_MISMATCHES_{}_UNKNOWN", bad, unknown) };
    format!("verify={}/{}:{} width={:.1}plies probe_us={:.1}", checked, positions.len(), status, width as f64 / decided.max(1) as f64, el * 1e6 / checked.max(1) as f64)
}

/// Times WDL probes (captures resolved, single thread, files in the page
/// cache: every position probed once before the timed pass) on random
/// positions of the named materials, own files against Syzygy's, and
/// compares the verdicts; DTM probes when the own dir has .dtm files.
pub fn egtb_speed(names: &str, own_dir: &str, syzygy_dir: &str, samples: usize) {
    let mut own = Tables::new();
    let n_own = own.add_own(Path::new(own_dir)).unwrap_or(0);
    let n_dtm = own.add_dtm(Path::new(own_dir)).unwrap_or(0);
    let syz = match Tables::open(Path::new(syzygy_dir)) {
        Ok(t) => t,
        Err(e) => {
            println!("syzygy: {}", e);
            return;
        }
    };
    println!("# egtb-speed {}: own {} WDL and {} DTM files in {}, syzygy {} files, {} positions per material", names, n_own, n_dtm, own_dir, syz.count(), samples);
    let (mut sum_own, mut sum_syz, mut sum_dtm, mut sum_n) = (0f64, 0f64, 0f64, 0usize);
    for m in materials_of(names) {
        let positions = random_positions(&m, samples, 0xD1B54A32D192ED03 ^ (m.name().len() as u64) << 7);
        let time = |f: &dyn Fn(&Board) -> Option<i32>| -> (f64, Vec<Option<i32>>) {
            let r: Vec<Option<i32>> = positions.iter().map(f).collect(); // warm: maps, page cache
            let t = Instant::now();
            let mut acc = 0i64;
            for b in &positions {
                acc += f(b).unwrap_or(9) as i64;
            }
            std::hint::black_box(acc);
            (t.elapsed().as_secs_f64() * 1e9 / positions.len() as f64, r)
        };
        let (ns_own, r_own) = time(&|b| own.probe_wdl(b));
        let (ns_syz, r_syz) = time(&|b| syz.probe_wdl(b));
        let missing = r_own.iter().filter(|v| v.is_none()).count();
        let differ = r_own.iter().zip(&r_syz).filter(|(a, b)| a.is_some() && b.is_some() && class_of_wdl(a.unwrap()) != class_of_wdl(b.unwrap())).count();
        let dtm = if n_dtm > 0 {
            let (ns, _) = time(&|b| own.probe_dtm(b).map(|(x, _)| x as i32));
            sum_dtm += ns;
            format!(" dtm_ns={:.0}", ns)
        } else {
            String::new()
        };
        println!("{} own_ns={:.0} syzygy_ns={:.0} ratio={:.2} missing={} differ={}{}", m.name(), ns_own, ns_syz, ns_own / ns_syz, missing, differ, dtm);
        sum_own += ns_own;
        sum_syz += ns_syz;
        sum_n += 1;
    }
    if sum_n > 0 {
        let n = sum_n as f64;
        println!("# mean over {} materials: own {:.0} ns, syzygy {:.0} ns, ratio {:.2}{}", sum_n, sum_own / n, sum_syz / n, sum_own / sum_syz, if n_dtm > 0 { format!(", dtm {:.0} ns", sum_dtm / n) } else { String::new() });
    }
}
