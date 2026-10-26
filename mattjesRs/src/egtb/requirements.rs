//! Measured generation costs per material and the measurement log, port of
//! `mattjesGo/egtb/requirements.go`: the engine tells before a generation
//! whether the machine can take it.

use std::path::Path;

use super::generate::Stats;
use super::peak::peak_memory;
use super::table::{Set, Table};

/// What a measured generation of a material cost: the peak committed memory
/// of the process, the time on the machine noted, and the compressed file
/// size. Recorded by hand from measure.log, like the checksums.
pub struct Requirement {
    pub peak_mb: usize,
    pub seconds: usize,
    pub file_mb: usize,
    pub machine: &'static str,
}

/// The measured materials beyond the base.
pub const REQUIREMENTS: &[(&str, Requirement)] = &[
    // five pieces, all 110: the sweep of 2026-10-23 on the home machine (Core Ultra 9 285H,
    // 14 workers, Rust, measure.log). The peak is the process peak since its start, which
    // only grows along the sweep (1.3 GB over the pawnless materials, 3.4 GB with one pawn,
    // 7.4 GB with two), so for a material it is an upper bound; the first material of every
    // plateau measured it. seconds: generation only; file_mb: the DTM file.
    ("KBBBK", Requirement { peak_mb: 707, seconds: 2, file_mb: 2, machine: "285H" }),
    ("KBBKB", Requirement { peak_mb: 914, seconds: 3, file_mb: 1, machine: "285H" }),
    ("KBBKN", Requirement { peak_mb: 1024, seconds: 6, file_mb: 10, machine: "285H" }),
    ("KBBKQ", Requirement { peak_mb: 1080, seconds: 13, file_mb: 11, machine: "285H" }),
    ("KBBKR", Requirement { peak_mb: 1082, seconds: 3, file_mb: 1, machine: "285H" }),
    ("KBBNK", Requirement { peak_mb: 1082, seconds: 7, file_mb: 7, machine: "285H" }),
    ("KBNKB", Requirement { peak_mb: 1082, seconds: 9, file_mb: 2, machine: "285H" }),
    ("KBNKN", Requirement { peak_mb: 1204, seconds: 10, file_mb: 5, machine: "285H" }),
    ("KBNKQ", Requirement { peak_mb: 1241, seconds: 28, file_mb: 25, machine: "285H" }),
    ("KBNKR", Requirement { peak_mb: 1328, seconds: 10, file_mb: 2, machine: "285H" }),
    ("KBNNK", Requirement { peak_mb: 1328, seconds: 6, file_mb: 4, machine: "285H" }),
    ("KNNKB", Requirement { peak_mb: 1328, seconds: 1, file_mb: 0, machine: "285H" }),
    ("KNNKN", Requirement { peak_mb: 1328, seconds: 1, file_mb: 0, machine: "285H" }),
    ("KNNKQ", Requirement { peak_mb: 1328, seconds: 13, file_mb: 10, machine: "285H" }),
    ("KNNKR", Requirement { peak_mb: 1328, seconds: 2, file_mb: 1, machine: "285H" }),
    ("KNNNK", Requirement { peak_mb: 1328, seconds: 2, file_mb: 1, machine: "285H" }),
    ("KQBBK", Requirement { peak_mb: 1328, seconds: 6, file_mb: 1, machine: "285H" }),
    ("KQBKB", Requirement { peak_mb: 1328, seconds: 19, file_mb: 7, machine: "285H" }),
    ("KQBKN", Requirement { peak_mb: 1328, seconds: 19, file_mb: 6, machine: "285H" }),
    ("KQBKQ", Requirement { peak_mb: 1328, seconds: 20, file_mb: 4, machine: "285H" }),
    ("KQBKR", Requirement { peak_mb: 1328, seconds: 20, file_mb: 11, machine: "285H" }),
    ("KQBNK", Requirement { peak_mb: 1328, seconds: 12, file_mb: 1, machine: "285H" }),
    ("KQNKB", Requirement { peak_mb: 1328, seconds: 18, file_mb: 8, machine: "285H" }),
    ("KQNKN", Requirement { peak_mb: 1328, seconds: 18, file_mb: 8, machine: "285H" }),
    ("KQNKQ", Requirement { peak_mb: 1328, seconds: 20, file_mb: 4, machine: "285H" }),
    ("KQNKR", Requirement { peak_mb: 1328, seconds: 20, file_mb: 11, machine: "285H" }),
    ("KQNNK", Requirement { peak_mb: 1328, seconds: 5, file_mb: 0, machine: "285H" }),
    ("KQQBK", Requirement { peak_mb: 1328, seconds: 6, file_mb: 0, machine: "285H" }),
    ("KQQKB", Requirement { peak_mb: 1328, seconds: 12, file_mb: 2, machine: "285H" }),
    ("KQQKN", Requirement { peak_mb: 1328, seconds: 11, file_mb: 2, machine: "285H" }),
    ("KQQKQ", Requirement { peak_mb: 1328, seconds: 10, file_mb: 3, machine: "285H" }),
    ("KQQKR", Requirement { peak_mb: 1328, seconds: 12, file_mb: 4, machine: "285H" }),
    ("KQQNK", Requirement { peak_mb: 1328, seconds: 6, file_mb: 0, machine: "285H" }),
    ("KQQQK", Requirement { peak_mb: 1328, seconds: 2, file_mb: 0, machine: "285H" }),
    ("KQQRK", Requirement { peak_mb: 1328, seconds: 6, file_mb: 1, machine: "285H" }),
    ("KQRBK", Requirement { peak_mb: 1328, seconds: 12, file_mb: 1, machine: "285H" }),
    ("KQRKB", Requirement { peak_mb: 1328, seconds: 21, file_mb: 3, machine: "285H" }),
    ("KQRKN", Requirement { peak_mb: 1328, seconds: 21, file_mb: 4, machine: "285H" }),
    ("KQRKQ", Requirement { peak_mb: 1328, seconds: 25, file_mb: 16, machine: "285H" }),
    ("KQRKR", Requirement { peak_mb: 1328, seconds: 22, file_mb: 6, machine: "285H" }),
    ("KQRNK", Requirement { peak_mb: 1328, seconds: 12, file_mb: 1, machine: "285H" }),
    ("KQRRK", Requirement { peak_mb: 1328, seconds: 6, file_mb: 0, machine: "285H" }),
    ("KRBBK", Requirement { peak_mb: 1328, seconds: 6, file_mb: 3, machine: "285H" }),
    ("KRBKB", Requirement { peak_mb: 1328, seconds: 18, file_mb: 13, machine: "285H" }),
    ("KRBKN", Requirement { peak_mb: 1328, seconds: 18, file_mb: 14, machine: "285H" }),
    ("KRBKQ", Requirement { peak_mb: 1328, seconds: 27, file_mb: 18, machine: "285H" }),
    ("KRBKR", Requirement { peak_mb: 1328, seconds: 12, file_mb: 3, machine: "285H" }),
    ("KRBNK", Requirement { peak_mb: 1328, seconds: 13, file_mb: 6, machine: "285H" }),
    ("KRNKB", Requirement { peak_mb: 1328, seconds: 18, file_mb: 13, machine: "285H" }),
    ("KRNKN", Requirement { peak_mb: 1328, seconds: 18, file_mb: 13, machine: "285H" }),
    ("KRNKQ", Requirement { peak_mb: 1328, seconds: 28, file_mb: 22, machine: "285H" }),
    ("KRNKR", Requirement { peak_mb: 1333, seconds: 12, file_mb: 2, machine: "285H" }),
    ("KRNNK", Requirement { peak_mb: 1333, seconds: 6, file_mb: 3, machine: "285H" }),
    ("KRRBK", Requirement { peak_mb: 1333, seconds: 6, file_mb: 1, machine: "285H" }),
    ("KRRKB", Requirement { peak_mb: 1333, seconds: 10, file_mb: 6, machine: "285H" }),
    ("KRRKN", Requirement { peak_mb: 1333, seconds: 10, file_mb: 5, machine: "285H" }),
    ("KRRKQ", Requirement { peak_mb: 1333, seconds: 13, file_mb: 8, machine: "285H" }),
    ("KRRKR", Requirement { peak_mb: 1333, seconds: 10, file_mb: 7, machine: "285H" }),
    ("KRRNK", Requirement { peak_mb: 1333, seconds: 6, file_mb: 1, machine: "285H" }),
    ("KRRRK", Requirement { peak_mb: 1333, seconds: 2, file_mb: 0, machine: "285H" }),
    ("KBBKP", Requirement { peak_mb: 2483, seconds: 19, file_mb: 18, machine: "285H" }),
    ("KBBPK", Requirement { peak_mb: 3050, seconds: 22, file_mb: 12, machine: "285H" }),
    ("KBNKP", Requirement { peak_mb: 3050, seconds: 57, file_mb: 65, machine: "285H" }),
    ("KBNPK", Requirement { peak_mb: 3413, seconds: 46, file_mb: 25, machine: "285H" }),
    ("KBPKB", Requirement { peak_mb: 3413, seconds: 37, file_mb: 18, machine: "285H" }),
    ("KBPKN", Requirement { peak_mb: 3413, seconds: 43, file_mb: 32, machine: "285H" }),
    ("KBPKQ", Requirement { peak_mb: 3413, seconds: 76, file_mb: 36, machine: "285H" }),
    ("KBPKR", Requirement { peak_mb: 3413, seconds: 50, file_mb: 19, machine: "285H" }),
    ("KNNKP", Requirement { peak_mb: 3413, seconds: 16, file_mb: 24, machine: "285H" }),
    ("KNNPK", Requirement { peak_mb: 3413, seconds: 20, file_mb: 11, machine: "285H" }),
    ("KNPKB", Requirement { peak_mb: 3413, seconds: 35, file_mb: 17, machine: "285H" }),
    ("KNPKN", Requirement { peak_mb: 3413, seconds: 39, file_mb: 28, machine: "285H" }),
    ("KNPKQ", Requirement { peak_mb: 3413, seconds: 74, file_mb: 48, machine: "285H" }),
    ("KNPKR", Requirement { peak_mb: 3413, seconds: 50, file_mb: 25, machine: "285H" }),
    ("KQBKP", Requirement { peak_mb: 3413, seconds: 55, file_mb: 6, machine: "285H" }),
    ("KQBPK", Requirement { peak_mb: 3413, seconds: 40, file_mb: 3, machine: "285H" }),
    ("KQNKP", Requirement { peak_mb: 3413, seconds: 53, file_mb: 13, machine: "285H" }),
    ("KQNPK", Requirement { peak_mb: 3413, seconds: 40, file_mb: 3, machine: "285H" }),
    ("KQPKB", Requirement { peak_mb: 3413, seconds: 61, file_mb: 33, machine: "285H" }),
    ("KQPKN", Requirement { peak_mb: 3413, seconds: 61, file_mb: 35, machine: "285H" }),
    ("KQPKQ", Requirement { peak_mb: 3413, seconds: 74, file_mb: 48, machine: "285H" }),
    ("KQPKR", Requirement { peak_mb: 3413, seconds: 79, file_mb: 54, machine: "285H" }),
    ("KQQKP", Requirement { peak_mb: 3413, seconds: 32, file_mb: 4, machine: "285H" }),
    ("KQQPK", Requirement { peak_mb: 3413, seconds: 19, file_mb: 1, machine: "285H" }),
    ("KQRKP", Requirement { peak_mb: 3413, seconds: 65, file_mb: 11, machine: "285H" }),
    ("KQRPK", Requirement { peak_mb: 3413, seconds: 39, file_mb: 3, machine: "285H" }),
    ("KRBKP", Requirement { peak_mb: 3413, seconds: 53, file_mb: 27, machine: "285H" }),
    ("KRBPK", Requirement { peak_mb: 3413, seconds: 43, file_mb: 19, machine: "285H" }),
    ("KRNKP", Requirement { peak_mb: 3413, seconds: 53, file_mb: 28, machine: "285H" }),
    ("KRNPK", Requirement { peak_mb: 3413, seconds: 43, file_mb: 22, machine: "285H" }),
    ("KRPKB", Requirement { peak_mb: 3413, seconds: 60, file_mb: 60, machine: "285H" }),
    ("KRPKN", Requirement { peak_mb: 3418, seconds: 63, file_mb: 57, machine: "285H" }),
    ("KRPKQ", Requirement { peak_mb: 3418, seconds: 90, file_mb: 88, machine: "285H" }),
    ("KRPKR", Requirement { peak_mb: 3432, seconds: 64, file_mb: 47, machine: "285H" }),
    ("KRRKP", Requirement { peak_mb: 3432, seconds: 32, file_mb: 7, machine: "285H" }),
    ("KRRPK", Requirement { peak_mb: 3432, seconds: 21, file_mb: 2, machine: "285H" }),
    ("KBPKP", Requirement { peak_mb: 6963, seconds: 44, file_mb: 30, machine: "285H" }),
    ("KBPPK", Requirement { peak_mb: 7353, seconds: 19, file_mb: 8, machine: "285H" }),
    ("KNPKP", Requirement { peak_mb: 7353, seconds: 43, file_mb: 35, machine: "285H" }),
    ("KNPPK", Requirement { peak_mb: 7357, seconds: 19, file_mb: 8, machine: "285H" }),
    ("KPPKB", Requirement { peak_mb: 7357, seconds: 18, file_mb: 7, machine: "285H" }),
    ("KPPKN", Requirement { peak_mb: 7357, seconds: 19, file_mb: 11, machine: "285H" }),
    ("KPPKQ", Requirement { peak_mb: 7357, seconds: 28, file_mb: 14, machine: "285H" }),
    ("KPPKR", Requirement { peak_mb: 7357, seconds: 28, file_mb: 20, machine: "285H" }),
    ("KQPKP", Requirement { peak_mb: 7357, seconds: 48, file_mb: 20, machine: "285H" }),
    ("KQPPK", Requirement { peak_mb: 7357, seconds: 18, file_mb: 1, machine: "285H" }),
    ("KRPKP", Requirement { peak_mb: 7357, seconds: 51, file_mb: 27, machine: "285H" }),
    ("KRPPK", Requirement { peak_mb: 7369, seconds: 19, file_mb: 6, machine: "285H" }),
    ("KPPKP", Requirement { peak_mb: 7369, seconds: 20, file_mb: 14, machine: "285H" }),
    ("KPPPK", Requirement { peak_mb: 7369, seconds: 6, file_mb: 2, machine: "285H" }),
    // six pieces, home machine (Core Ultra 9 285H, 14 workers), Rust, UCI mode
    ("KRRKBN", Requirement { peak_mb: 27450, seconds: 1348, file_mb: 2427, machine: "285H" }),
];

pub fn requirement(name: &str) -> Option<&'static Requirement> {
    REQUIREMENTS.iter().find(|(n, _)| *n == name).map(|(_, r)| r)
}

/// The measurement log in the cache directory, one line per generated table
/// (egtb-measure and the UCI background generation).
pub const LOG_FILE_NAME: &str = "measure.log";

/// Appends one line to the measurement log of a cache directory.
pub fn append_log(cache_dir: &Path, line: &str) -> std::io::Result<()> {
    use std::io::Write;
    std::fs::create_dir_all(cache_dir)?;
    let mut f = std::fs::OpenOptions::new().append(true).create(true).open(cache_dir.join(LOG_FILE_NAME))?;
    writeln!(f, "{}", line)
}

impl Set {
    /// The memory a generation of the table needs: the measured peak when
    /// recorded, otherwise an estimate from the table size (table, four
    /// bitsets of 1/8, pending lists of about 1/4 as seen on KRRKBN) plus the
    /// direct dependencies that are not loaded yet. The bool says "measured".
    pub fn estimate_bytes(&self, ti: usize) -> (usize, bool) {
        let t = &self.tables[ti];
        if let Some(r) = requirement(&t.mat.name()) {
            return (r.peak_mb << 20, true);
        }
        let mut n = t.size + t.size / 2 + t.size / 4;
        for d in t.mat.dependencies() {
            match self.find(&d.name()) {
                Some(i) if self.tables[i].is_generated() => {}
                _ => n += Table::new(d).size,
            }
        }
        (n, false)
    }

    /// The measurement log line of a generated table (the same fields from
    /// egtb-measure and from the UCI mode).
    pub fn log_line(&self, ti: usize, st: &Stats, workers: usize, source: &str) -> String {
        let t = &self.tables[ti];
        let (peak_commit, peak_ws) = peak_memory().unwrap_or((0, 0));
        format!(
            "{} pieces={} indices={} legal={} wins={} losses={} draws={} longest_plies={} levels={} evaluations={} seconds={:.1} workers={} overflow={} beyond={} wide={} peak_commit_mb={} peak_ws_mb={} checksum={:016x} {}",
            t.mat.name(),
            t.mat.pieces(),
            t.size,
            st.legal,
            st.wins,
            st.losses,
            st.draws,
            st.max_win,
            st.levels,
            st.evaluations,
            st.duration.as_secs_f64(),
            workers,
            st.overflow,
            st.beyond,
            st.wide,
            peak_commit >> 20,
            peak_ws >> 20,
            t.raw_checksum(),
            source
        )
    }
}
