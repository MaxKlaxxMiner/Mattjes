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
    // five pieces, work machine (i5, 12 threads), base loaded
    ("KBBBK", Requirement { peak_mb: 616, seconds: 4, file_mb: 11, machine: "i5" }),
    ("KBNKQ", Requirement { peak_mb: 744, seconds: 49, file_mb: 109, machine: "i5" }),
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
            "{} pieces={} indices={} legal={} wins={} losses={} draws={} longest_plies={} levels={} evaluations={} seconds={:.1} workers={} overflow={} beyond={} peak_commit_mb={} peak_ws_mb={} checksum={:016x} {}",
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
            peak_commit >> 20,
            peak_ws >> 20,
            t.raw_checksum(),
            source
        )
    }
}
