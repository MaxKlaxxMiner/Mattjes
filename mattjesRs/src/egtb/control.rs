//! Pause or abort a running generation from another thread, port of
//! `mattjesGo/egtb/control.go`. The workers check it before every chunk of
//! work, so a pause takes effect within milliseconds and a paused generation
//! resumes exactly where it stopped, with everything computed so far kept in
//! memory.

use std::sync::{Condvar, Mutex};

#[derive(Default)]
struct State {
    paused: bool,
    aborted: bool,
}

#[derive(Default)]
pub struct Control {
    state: Mutex<State>,
    cond: Condvar,
}

impl Control {
    pub fn new() -> Control {
        Control::default()
    }

    /// Stops the workers at the next chunk boundary.
    pub fn pause(&self) {
        self.state.lock().unwrap().paused = true;
    }

    /// Lets paused workers continue.
    pub fn resume(&self) {
        self.state.lock().unwrap().paused = false;
        self.cond.notify_all();
    }

    /// Ends the generation; its result is incomplete and must be dropped.
    pub fn abort(&self) {
        let mut s = self.state.lock().unwrap();
        s.aborted = true;
        s.paused = false;
        drop(s);
        self.cond.notify_all();
    }

    pub fn paused(&self) -> bool {
        self.state.lock().unwrap().paused
    }

    pub fn aborted(&self) -> bool {
        self.state.lock().unwrap().aborted
    }

    /// Blocks while paused; false means aborted.
    pub(super) fn wait(&self) -> bool {
        let mut s = self.state.lock().unwrap();
        while s.paused && !s.aborted {
            s = self.cond.wait(s).unwrap();
        }
        !s.aborted
    }
}
