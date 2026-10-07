//! Minimal LZ77 compressor for the endgame table cache, direct port of
//! `mattjesGo/lz` (the author's own design from webwerfgo/lz.go): LZ4 block
//! format, hash chains with lazy matching for the encoder, independent 4 MB
//! blocks compressed and decompressed in parallel.
//!
//! Container: raw_len u64, block_size u32, block_count u32, then per block
//! comp_len u32 and the payload (all little endian).

pub const MIN_MATCH: usize = 4;
pub const HASH_BITS: u32 = 16;
pub const MAX_OFFSET: usize = (1 << 16) - 1;
pub const BLOCK_SIZE: usize = 4 << 20;
/// Hash chain candidates tried per position.
pub const DEPTH: usize = 256;

const HEADER_BYTES: usize = 16;

#[inline(always)]
fn hash(v: u32) -> usize {
    (v.wrapping_mul(2654435761) >> (32 - HASH_BITS)) as usize
}

#[inline(always)]
fn le32(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn compress_block(src: &[u8], depth: usize) -> Vec<u8> {
    let mut head = vec![-1i32; 1 << HASH_BITS];
    let mut prev = vec![0i32; src.len()];
    let mut dst: Vec<u8> = Vec::with_capacity(src.len() / 3 + 16);

    fn write_len(dst: &mut Vec<u8>, mut n: usize) {
        while n >= 255 {
            dst.push(255);
            n -= 255;
        }
        dst.push(n as u8);
    }
    fn emit(dst: &mut Vec<u8>, lit: &[u8], mlen: usize, ofs: usize) {
        let ll = lit.len();
        let ml = if mlen > 0 { mlen - MIN_MATCH } else { 0 };
        dst.push(((ll.min(15) as u8) << 4) | ml.min(15) as u8);
        if ll >= 15 {
            write_len(dst, ll - 15);
        }
        dst.extend_from_slice(lit);
        if mlen > 0 {
            dst.push(ofs as u8);
            dst.push((ofs >> 8) as u8);
            if ml >= 15 {
                write_len(dst, ml - 15);
            }
        }
    }
    let limit = src.len() as isize - 5;
    let insert = |head: &mut [i32], prev: &mut [i32], p: usize| {
        let h = hash(le32(&src[p..]));
        prev[p] = head[h];
        head[h] = p as i32;
    };
    let find = |head: &[i32], prev: &[i32], p: usize| -> (usize, usize) {
        let (mut best_len, mut best_ofs) = (0usize, 0usize);
        let mut cand = head[hash(le32(&src[p..]))];
        let mut d = 0;
        while d < depth && cand >= 0 && p - cand as usize <= MAX_OFFSET {
            let c = cand as usize;
            if src[c + best_len] == src[p + best_len] && le32(&src[c..]) == le32(&src[p..]) {
                let mut l = MIN_MATCH;
                while ((p + l) as isize) < limit && src[c + l] == src[p + l] {
                    l += 1;
                }
                if l > best_len {
                    best_len = l;
                    best_ofs = p - c;
                }
            }
            cand = prev[c];
            d += 1;
        }
        (best_len, best_ofs)
    };

    let (mut lit_start, mut p) = (0usize, 0usize);
    while ((p + MIN_MATCH) as isize) <= limit {
        let (best_len, best_ofs) = find(&head, &prev, p);
        if best_len < MIN_MATCH {
            insert(&mut head, &mut prev, p);
            p += 1;
            continue;
        }
        let mut from = 0;
        if ((p + 1 + MIN_MATCH) as isize) <= limit {
            // lazy: prefer a longer match one byte later
            insert(&mut head, &mut prev, p);
            from = 1;
            let (l2, _) = find(&head, &prev, p + 1);
            if l2 > best_len {
                p += 1;
                continue;
            }
        }
        emit(&mut dst, &src[lit_start..p], best_len, best_ofs);
        for i in from..best_len {
            insert(&mut head, &mut prev, p + i);
        }
        p += best_len;
        lit_start = p;
    }
    emit(&mut dst, &src[lit_start..], 0, 0);
    dst
}

/// Decodes src into dst, which must have exactly the raw length.
fn decompress_block(dst: &mut [u8], src: &[u8]) -> Result<(), String> {
    let (mut s, mut d) = (0usize, 0usize);
    let read_len = |s: &mut usize, mut n: usize| -> Option<usize> {
        while *s < src.len() && src[*s] == 255 {
            n += 255;
            *s += 1;
        }
        if *s >= src.len() {
            return None;
        }
        n += src[*s] as usize;
        *s += 1;
        Some(n)
    };
    while s < src.len() {
        let tok = src[s] as usize;
        s += 1;
        let mut ll = tok >> 4;
        if ll == 15 {
            ll = read_len(&mut s, 15).ok_or("lz: truncated literal length")?;
        }
        if s + ll > src.len() || d + ll > dst.len() {
            return Err("lz: literals out of range".into());
        }
        dst[d..d + ll].copy_from_slice(&src[s..s + ll]);
        s += ll;
        d += ll;
        if s >= src.len() {
            break;
        }
        if s + 2 > src.len() {
            return Err("lz: truncated offset".into());
        }
        let ofs = src[s] as usize | (src[s + 1] as usize) << 8;
        s += 2;
        let mut ml = (tok & 15) + MIN_MATCH;
        if ml == 15 + MIN_MATCH {
            ml = read_len(&mut s, ml).ok_or("lz: truncated match length")?;
        }
        if ofs == 0 || ofs > d || d + ml > dst.len() {
            return Err("lz: match out of range".into());
        }
        let m = d - ofs;
        if ofs >= ml {
            dst.copy_within(m..m + ml, d);
        } else {
            // overlapping match (a run), must copy forward byte by byte
            for i in 0..ml {
                dst[d + i] = dst[m + i];
            }
        }
        d += ml;
    }
    if d != dst.len() {
        return Err("lz: block length mismatch".into());
    }
    Ok(())
}

/// Runs f for every block index on `workers` threads.
fn parallel_for(n: usize, workers: usize, f: &(dyn Fn(usize) + Sync)) {
    let workers = workers.max(1).min(n.max(1));
    let next = std::sync::atomic::AtomicUsize::new(0);
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if i >= n {
                    return;
                }
                f(i);
            });
        }
    });
}

/// Compresses data in independent blocks, in parallel.
pub fn pack(data: &[u8], workers: usize) -> Vec<u8> {
    let n = data.len().div_ceil(BLOCK_SIZE);
    let blocks: Vec<std::sync::Mutex<Vec<u8>>> = (0..n).map(|_| std::sync::Mutex::new(Vec::new())).collect();
    parallel_for(n, workers, &|i| {
        let end = ((i + 1) * BLOCK_SIZE).min(data.len());
        *blocks[i].lock().unwrap() = compress_block(&data[i * BLOCK_SIZE..end], DEPTH);
    });
    let mut out = Vec::with_capacity(HEADER_BYTES + data.len() / 3);
    out.extend_from_slice(&(data.len() as u64).to_le_bytes());
    out.extend_from_slice(&(BLOCK_SIZE as u32).to_le_bytes());
    out.extend_from_slice(&(n as u32).to_le_bytes());
    for b in &blocks {
        let b = b.lock().unwrap();
        out.extend_from_slice(&(b.len() as u32).to_le_bytes());
        out.extend_from_slice(&b);
    }
    out
}

/// The decompressed length stored in a container.
pub fn raw_len(buf: &[u8]) -> Result<usize, String> {
    if buf.len() < HEADER_BYTES {
        return Err("lz: short header".into());
    }
    Ok(u64::from_le_bytes(buf[..8].try_into().unwrap()) as usize)
}

/// Decompresses into dst, which must have exactly the raw length.
pub fn unpack_into(dst: &mut [u8], buf: &[u8], workers: usize) -> Result<(), String> {
    let total = raw_len(buf)?;
    let block = u32::from_le_bytes(buf[8..12].try_into().unwrap()) as usize;
    let n = u32::from_le_bytes(buf[12..16].try_into().unwrap()) as usize;
    if total != dst.len() || block == 0 || n != total.div_ceil(block) {
        return Err("lz: bad header".into());
    }
    let mut blocks = Vec::with_capacity(n);
    let mut p = HEADER_BYTES;
    for _ in 0..n {
        if p + 4 > buf.len() {
            return Err("lz: truncated".into());
        }
        let l = u32::from_le_bytes(buf[p..p + 4].try_into().unwrap()) as usize;
        p += 4;
        if p + l > buf.len() {
            return Err("lz: truncated".into());
        }
        blocks.push(&buf[p..p + l]);
        p += l;
    }
    // hand every thread its own output slice
    let parts: Vec<std::sync::Mutex<&mut [u8]>> = dst.chunks_mut(block).map(std::sync::Mutex::new).collect();
    let errors: Vec<std::sync::Mutex<Option<String>>> = (0..n).map(|_| std::sync::Mutex::new(None)).collect();
    parallel_for(n, workers, &|i| {
        let mut part = parts[i].lock().unwrap();
        if let Err(e) = decompress_block(&mut part, blocks[i]) {
            *errors[i].lock().unwrap() = Some(e);
        }
    });
    for e in errors {
        if let Some(e) = e.into_inner().unwrap() {
            return Err(e);
        }
    }
    Ok(())
}

/// The inverse of `pack`.
pub fn unpack(buf: &[u8], workers: usize) -> Result<Vec<u8>, String> {
    let mut dst = vec![0u8; raw_len(buf)?];
    unpack_into(&mut dst, buf, workers)?;
    Ok(dst)
}
