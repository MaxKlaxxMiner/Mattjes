package uci

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/egtb"
	"github.com/MaxKlaxxMiner/Mattjes/mattjesGo/syzygy"
)

// The sweep (option EgtbForceGen): "go infinite" generates every material
// the options allow (EgtbGenerate5, EgtbGenerate6) one after the other on
// every thread of the Threads option, instead of a search; "stop" pauses it,
// the next "go infinite" resumes it. The product of a material is its WDL
// file (unless the Syzygy files cover it) and its DTM file; nothing stays in
// memory. The exact file (RawPath) of a material is written only while a
// later material of the plan still depends on it, and removed as soon as none
// does, so that the disk holds the exact tables only as long as the
// generation needs them.

// registerOwnFiles adds the own WDL and DTM files the cache folder gained
// (a finished job, or a paused sweep) to the search oracle; both calls skip
// what is registered already. The searches must be stopped: they read the
// tables.
func (e *Engine) registerOwnFiles() {
	if e.syzygy == nil {
		return
	}
	if n, err := e.syzygy.AddOwn(e.set.WdlPath()); err == nil && n > 0 {
		e.send("info string wdl: %d own tables added", n)
	}
	if n, err := e.syzygy.AddDTM(e.set.DtmPath()); err == nil && n > 0 {
		e.send("info string dtm: %d own tables added", n)
	}
}

// hasProduct reports whether the files of a material beyond the base are
// complete: the DTM file, and the WDL file unless the Syzygy files cover the
// material (then no own WDL is written, see writeWdl).
func (e *Engine) hasProduct(m egtb.Material) bool { return productOf(e.set, m, e.syzygy) }

func productOf(set *egtb.Set, m egtb.Material, syz *syzygy.Tables) bool {
	if fileMissing(set.DtmFile(m)) {
		return false
	}
	return !fileMissing(set.WdlFile(m)) || syz != nil && syz.HasOriginal(m)
}

// sweepPlan lists the materials a sweep generates, in dependency order: every
// five-piece material without its files when EgtbGenerate5 is set, then the
// six-piece ones when EgtbGenerate6 is set (their captures lead into the
// five-piece ones).
func (e *Engine) sweepPlan() []egtb.Material {
	var plan []egtb.Material
	for _, n := range []int{5, 6} {
		if n == 5 && !e.generate5 || n == 6 && !e.generate6 {
			continue
		}
		for _, m := range egtb.Enumerate(n) {
			if !e.hasProduct(m) {
				plan = append(plan, m)
			}
		}
	}
	return plan
}

// startSweep resumes a paused sweep, or starts one on a fork of the set; a
// running single generation is dropped for it. False when every material
// the options allow has its files: the go then runs as usual.
func (e *Engine) startSweep() bool {
	if e.job != nil && e.job.sweep {
		if e.job.ctrl.Paused() {
			e.job.ctrl.Resume()
			e.send("info string egtb: sweep resumed")
		}
		return true
	}
	plan := e.sweepPlan()
	if len(plan) == 0 {
		e.send("info string egtb: sweep: every material the options allow has its files")
		return false
	}
	if e.job != nil {
		e.job.ctrl.Abort()
		e.send("info string egtb: generation of %s dropped for the sweep", e.job.mat.Name())
		e.job = nil
	}
	if e.tableMB > 0 {
		e.send("info string egtb: the search's hash table (%d MB) is released for the sweep", e.tableMB)
		e.dropTables()
	}
	set := e.set.Fork()
	j := &job{mat: plan[0], ctrl: egtb.NewControl(), sweep: true}
	e.job = j
	workers, syz := e.workers, e.syzygy
	e.send("info string egtb: sweep: %d materials to generate on %d threads (%s first), stop pauses it", len(plan), workers, plan[0].Name())
	go e.runSweep(set, j, plan, workers, syz)
	return true
}

// neededLater reports whether a material of the plan's rest looks m up
// (captures and promotions lead into it): its exact file is needed then.
func neededLater(m egtb.Material, rest []egtb.Material) bool {
	for _, r := range rest {
		for _, d := range r.Dependencies() {
			if d.Name() == m.Name() {
				return true
			}
		}
	}
	return false
}

// saveRaw writes the exact file of a table when it is missing.
func saveRaw(set *egtb.Set, t *egtb.Table, progress func(string)) {
	path := set.RawPath(t)
	if !fileMissing(path) {
		return
	}
	if err := set.SaveRawTable(t, path); err != nil {
		progress(err.Error())
		return
	}
	progress("written exact to " + path)
}

// cleanRaw removes every exact file whose material has its product and is
// not looked up by any material of the plan's rest, and the leftovers of
// interrupted writes (TmpPath; nothing is being written at this point).
func cleanRaw(set *egtb.Set, rest []egtb.Material, syz *syzygy.Tables, progress func(string)) {
	for _, dir := range []string{set.WdlPath(), set.DtmPath()} {
		names, _ := os.ReadDir(dir)
		for _, de := range names {
			if !de.IsDir() && strings.HasSuffix(de.Name(), ".tmp") {
				os.Remove(filepath.Join(dir, de.Name()))
			}
		}
	}
	names, err := os.ReadDir(set.DtmPath())
	if err != nil {
		return
	}
	for _, de := range names {
		if de.IsDir() || !strings.HasSuffix(de.Name(), ".bin"+egtb.RawSuffix) {
			continue
		}
		m, ok := egtb.ParseFileName(de.Name())
		if !ok || !productOf(set, m, syz) || neededLater(m, rest) {
			continue
		}
		path := filepath.Join(set.DtmPath(), de.Name())
		if err := os.Remove(path); err != nil {
			progress(err.Error())
			continue
		}
		progress("removed " + path + ", no remaining material needs it")
	}
}

// runSweep is the sweep's thread: material by material, the exact file
// loaded when it exists, generated otherwise (the generator loads or computes
// the dependencies), the WDL and DTM files written by the done callback, the
// exact file written only for a material a later one needs, then everything
// released from memory and the exact files nobody needs any more removed.
func (e *Engine) runSweep(set *egtb.Set, j *job, plan []egtb.Material, workers int, syz *syzygy.Tables) {
	// every line of the running material carries its number in the plan,
	// "(3 / 109) ...", like the base generation's "(3 / 35)"
	var lastLevel time.Time
	current := 0
	progress := func(line string) {
		if strings.HasPrefix(line, " ") {
			if time.Since(lastLevel) < progressInterval {
				return
			}
			lastLevel = time.Now()
		}
		e.send("info string egtb: (%d / %d) %s", current, len(plan), compact(strings.TrimPrefix(line, "egtb: ")))
		e.flush()
	}
	start := time.Now()
	for i, m := range plan {
		if !j.ctrl.Wait() {
			return // aborted between two materials
		}
		current = i + 1
		rest := plan[i+1:]
		t := set.AddMaterial(m)
		need, measured := set.EstimateBytes(t)
		how := "estimated"
		if measured {
			how = "measured"
		}
		if total, _, ok := egtb.AvailableMemory(); ok && uint64(need) > total {
			progress(fmt.Sprintf("%s needs about %s (%s), the machine has %s: skipped", m.Name(), egtb.FormatBytes(uint64(need)), how, egtb.FormatBytes(total)))
			continue
		}
		progress(fmt.Sprintf("generating %s (%d MB table, about %s RAM %s)", m.Name(), t.Size>>20, egtb.FormatBytes(uint64(need)), how))
		// every computed table gets its log line and its files; a dependency
		// the generator had to compute (its exact file was gone) keeps an
		// exact file only when a later material of the plan needs it
		done := func(gt *egtb.Table, st egtb.Stats) {
			if !st.Loaded {
				if err := egtb.AppendLog(set.CacheDir, set.LogLine(gt, st, workers, "uci")); err != nil {
					progress(err.Error())
				}
			}
			writeWdl(set, gt, syz, workers, progress)
			writeDtm(set, gt, workers, progress)
			if gt != t && neededLater(gt.Mat, rest) {
				saveRaw(set, gt, progress)
			}
		}
		loaded := false
		if raw := set.RawPath(t); !fileMissing(raw) {
			at := time.Now()
			if err := set.LoadRawTable(t, raw); err != nil {
				progress(err.Error())
			} else {
				progress(fmt.Sprintf("loaded exact %s (%d MB) in %.1f s", raw, t.Size>>20, time.Since(at).Seconds()))
				loaded = true
				done(t, egtb.Stats{Loaded: true})
			}
		}
		if !loaded {
			if st := set.GenerateNotify(t, workers, j.ctrl, progress, done); st.Aborted {
				return
			}
		}
		if neededLater(m, rest) {
			saveRaw(set, t, progress)
		}
		// nothing stays in memory: the files are the product, the next
		// material loads its dependencies from their exact files
		for _, gt := range set.Tables[len(set.Base()):] {
			if gt.Generated() {
				set.Release(gt)
			}
		}
		cleanRaw(set, rest, syz, progress)
		progress(fmt.Sprintf("%s done, %s since the start", m.Name(), time.Since(start).Round(time.Second)))
	}
	e.jobDone <- set
}
