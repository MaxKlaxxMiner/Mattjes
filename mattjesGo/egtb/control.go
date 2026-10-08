package egtb

import "sync"

// Control pauses or aborts a running generation from another goroutine. The
// workers check it before every chunk of work, so a pause takes effect within
// milliseconds and a paused generation resumes exactly where it stopped, with
// everything computed so far kept in memory. A nil Control never pauses.
type Control struct {
	mu      sync.Mutex
	cond    *sync.Cond
	paused  bool
	aborted bool
}

func NewControl() *Control {
	c := &Control{}
	c.cond = sync.NewCond(&c.mu)
	return c
}

// Pause stops the workers at the next chunk boundary.
func (c *Control) Pause() {
	c.mu.Lock()
	c.paused = true
	c.mu.Unlock()
}

// Resume lets paused workers continue.
func (c *Control) Resume() {
	c.mu.Lock()
	c.paused = false
	c.mu.Unlock()
	c.cond.Broadcast()
}

// Abort ends the generation; its result is incomplete and must be dropped.
func (c *Control) Abort() {
	c.mu.Lock()
	c.aborted = true
	c.paused = false
	c.mu.Unlock()
	c.cond.Broadcast()
}

func (c *Control) Paused() bool {
	if c == nil {
		return false
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.paused
}

func (c *Control) Aborted() bool {
	if c == nil {
		return false
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.aborted
}

// wait blocks while paused; false means aborted.
func (c *Control) wait() bool {
	if c == nil {
		return true
	}
	c.mu.Lock()
	for c.paused && !c.aborted {
		c.cond.Wait()
	}
	ok := !c.aborted
	c.mu.Unlock()
	return ok
}
