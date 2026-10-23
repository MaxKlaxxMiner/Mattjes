//! The sweep (option EgtbForceGen), port of `mattjesGo/uci/sweep.go`: "go
//! infinite" generates every material the options allow (EgtbGenerate5,
//! EgtbGenerate6) one after the other on every thread of the Threads option,
//! instead of a search; "stop" pauses it, the next "go infinite" resumes it.
//! The product of a material is its WDL file (unless the Syzygy files cover
//! it) and its DTM file; nothing stays in memory. The exact file (`raw_path`)
//! of a material is written only while a later material of the plan still
//! depends on it, and removed as soon as none does, so that the disk holds
//! the exact tables only as long as the generation needs them.

use std::io::Write;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use super::{compact, write_dtm_file, write_wdl_file, Engine, Event, Job, PROGRESS_INTERVAL};
use crate::egtb::{self, Control, Material, Set};
use crate::syzygy::Tables;

/// Whether the files of a material beyond the base are complete: the DTM
/// file, and the WDL file unless the Syzygy files cover the material (then
/// no own WDL is written, see `write_wdl_file`).
pub(super) fn product_of(set: &Set, m: &Material, syz: Option<&Tables>) -> bool {
    if !set.dtm_file(m).exists() {
        return false;
    }
    set.wdl_file(m).exists() || syz.is_some_and(|t| t.has_original(m))
}

/// Whether a material of the plan's rest looks m up (captures and
/// promotions lead into it): its exact file is needed then.
fn needed_later(m: &Material, rest: &[Material]) -> bool {
    let name = m.name();
    rest.iter().any(|r| r.dependencies().iter().any(|d| d.name() == name))
}

/// Writes the exact file of a table when it is missing.
fn save_raw(set: &Set, ti: usize, say: &dyn Fn(&str)) {
    let path = set.raw_path(&set.tables[ti]);
    if path.exists() {
        return;
    }
    match set.save_raw_table(ti, &path) {
        Ok(()) => say(&format!("written exact to {}", path.display())),
        Err(e) => say(&e.to_string()),
    }
}

/// Removes every exact file whose material has its product and is not
/// looked up by any material of the plan's rest, and the leftovers of
/// interrupted writes (`tmp_path`; nothing is being written at this point).
fn clean_raw(set: &Set, rest: &[Material], syz: Option<&Tables>, say: &dyn Fn(&str)) {
    for dir in [set.wdl_path(), set.dtm_path()] {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for de in entries.flatten() {
            if !de.path().is_dir() && de.file_name().to_string_lossy().ends_with(".tmp") {
                let _ = std::fs::remove_file(de.path());
            }
        }
    }
    let Ok(dir) = std::fs::read_dir(set.dtm_path()) else { return };
    let suffix = format!(".bin{}", egtb::RAW_SUFFIX);
    for de in dir.flatten() {
        let name = de.file_name().to_string_lossy().into_owned();
        if !name.ends_with(&suffix) || de.path().is_dir() {
            continue;
        }
        let Some(m) = Set::parse_file_name(&name) else { continue };
        if !product_of(set, &m, syz) || needed_later(&m, rest) {
            continue;
        }
        let path = de.path();
        match std::fs::remove_file(&path) {
            Ok(()) => say(&format!("removed {}, no remaining material needs it", path.display())),
            Err(e) => say(&e.to_string()),
        }
    }
}

impl<W: Write + Send + 'static> Engine<W> {
    /// Whether the files of a material beyond the base are complete.
    pub(super) fn has_product(&self, m: &Material) -> bool {
        product_of(self.set(), m, self.syzygy.as_deref())
    }

    /// The materials a sweep generates, in dependency order: every five-piece
    /// material without its files when EgtbGenerate5 is set, then the
    /// six-piece ones when EgtbGenerate6 is set (their captures lead into the
    /// five-piece ones).
    fn sweep_plan(&self) -> Vec<Material> {
        let mut plan = Vec::new();
        for n in [5usize, 6] {
            if n == 5 && !self.generate5 || n == 6 && !self.generate6 {
                continue;
            }
            for m in Material::all_with(n) {
                if !self.has_product(&m) {
                    plan.push(m);
                }
            }
        }
        plan
    }

    /// Resumes a paused sweep, or starts one on a fork of the set; a running
    /// single generation is dropped for it. False when every material the
    /// options allow has its files: the go then runs as usual.
    pub(super) fn start_sweep(&mut self) -> bool {
        if let Some(j) = &self.job {
            if j.sweep {
                if j.ctrl.paused() {
                    j.ctrl.resume();
                    self.send("info string egtb: sweep resumed");
                }
                return true;
            }
        }
        let plan = self.sweep_plan();
        if plan.is_empty() {
            self.send("info string egtb: sweep: every material the options allow has its files");
            return false;
        }
        if let Some(j) = self.job.take() {
            j.ctrl.abort();
            self.send(&format!("info string egtb: generation of {} dropped for the sweep", j.mat.name()));
        }
        if self.table_mb > 0 {
            self.send(&format!("info string egtb: the search's hash table ({} MB) is released for the sweep", self.table_mb));
            self.drop_tables();
        }
        let fork = self.set().fork();
        let ctrl = Arc::new(Control::new());
        self.job = Some(Job { mat: plan[0].clone(), ctrl: ctrl.clone(), sweep: true });
        let workers = self.workers;
        let syz = self.syzygy.clone();
        let (out, tx) = (self.out.clone(), self.tx.clone());
        self.send(&format!("info string egtb: sweep: {} materials to generate on {} threads ({} first), stop pauses it", plan.len(), workers, plan[0].name()));
        std::thread::spawn(move || run_sweep(fork, ctrl, plan, workers, syz, out, tx));
        true
    }
}

/// The sweep's thread: material by material, the exact file loaded when it
/// exists, generated otherwise (the generator loads or computes the
/// dependencies), the WDL and DTM files written by the done callback, the
/// exact file written only for a material a later one needs, then everything
/// released from memory and the exact files nobody needs any more removed.
fn run_sweep<W: Write + Send + 'static>(mut set: Set, ctrl: Arc<Control>, plan: Vec<Material>, workers: usize, syz: Option<Arc<Tables>>, out: Arc<Mutex<W>>, tx: mpsc::Sender<Event>) {
    // every line of the running material carries its number in the plan,
    // "(3 / 109) ...", like the base generation's "(3 / 35)"
    let current = std::cell::Cell::new(0usize);
    let n = plan.len();
    let say = |line: &str| {
        let mut o = out.lock().unwrap();
        let _ = writeln!(o, "info string egtb: ({} / {}) {}", current.get(), n, compact(line.strip_prefix("egtb: ").unwrap_or(line)));
        let _ = o.flush();
    };
    // level lines come up to 254 times per table: at most one every few seconds
    let mut last_level: Option<Instant> = None;
    let mut progress = |line: &str| {
        if line.starts_with(' ') {
            if last_level.is_some_and(|t| t.elapsed() < PROGRESS_INTERVAL) {
                return;
            }
            last_level = Some(Instant::now());
        }
        say(line);
    };
    let start = Instant::now();
    for (i, m) in plan.iter().enumerate() {
        if !ctrl.wait() {
            return; // aborted between two materials
        }
        current.set(i + 1);
        let rest = &plan[i + 1..];
        let ti = set.add_material(m.clone());
        let size = set.tables[ti].size;
        let (need, measured) = set.estimate_bytes(ti);
        let how = if measured { "measured" } else { "estimated" };
        if let Some((total, _)) = egtb::available_memory() {
            if need as u64 > total {
                say(&format!("{} needs about {} ({}), the machine has {}: skipped", m.name(), egtb::format_bytes(need as u64), how, egtb::format_bytes(total)));
                continue;
            }
        }
        say(&format!("generating {} ({} MB table, about {} RAM {})", m.name(), size >> 20, egtb::format_bytes(need as u64), how));
        // every computed table gets its log line and its files; a dependency
        // the generator had to compute (its exact file was gone) keeps an
        // exact file only when a later material of the plan needs it
        let mut done = |set: &mut Set, gi: usize, st: &egtb::Stats| {
            if !st.loaded {
                if let Err(e) = egtb::append_log(&set.cache_dir, &set.log_line(gi, st, workers, "uci")) {
                    say(&e.to_string());
                }
            }
            write_wdl_file(set, gi, syz.as_deref(), workers, &say);
            write_dtm_file(set, gi, workers, &say);
            if gi != ti && needed_later(&set.tables[gi].mat, rest) {
                save_raw(set, gi, &say);
            }
        };
        let mut loaded = false;
        let raw = set.raw_path(&set.tables[ti]);
        if raw.exists() {
            let at = Instant::now();
            match set.load_raw_table(ti, &raw) {
                Err(e) => say(&e.to_string()),
                Ok(()) => {
                    say(&format!("loaded exact {} ({} MB) in {:.1} s", raw.display(), size >> 20, at.elapsed().as_secs_f64()));
                    loaded = true;
                    done(&mut set, ti, &egtb::Stats { loaded: true, ..Default::default() });
                }
            }
        }
        if !loaded {
            let st = set.generate_notify(ti, workers, Some(&ctrl), &mut progress, Some(&mut done));
            if st.aborted {
                return;
            }
        }
        if needed_later(m, rest) {
            save_raw(&set, ti, &say);
        }
        // nothing stays in memory: the files are the product, the next
        // material loads its dependencies from their exact files
        for gi in set.base().len()..set.tables.len() {
            if set.tables[gi].is_available() {
                set.release(gi);
            }
        }
        clean_raw(&set, rest, syz.as_deref(), &say);
        say(&format!("{} done, {:?} since the start", m.name(), Duration::from_secs(start.elapsed().as_secs())));
    }
    let _ = tx.send(Event::Done(set));
}
