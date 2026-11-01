// Package pending stores the single confirmed-later action of the gateway.
//
// The store lives in memory on purpose: a pending action is short-lived, is
// bound to one process, and must never survive a restart. Keeping it server
// side makes infra_confirm work even when the voice client loses history.
package pending

import (
	"sync"
	"time"
)

// Action is the only mutation the gateway performs: a rollout restart.
type Action struct {
	// ServiceID is the catalog id the action targets.
	ServiceID string
	// Namespace, WorkloadKind and WorkloadName identify the object to patch.
	Namespace    string
	WorkloadKind string
	WorkloadName string
	// Kind is always "rollout-restart" today; kept explicit for audit output.
	Kind string
	// SelfDisrupting defers execution so the reply can be spoken first.
	SelfDisrupting bool
	// Delay is used when SelfDisrupting is true.
	Delay time.Duration
	// Warning is the catalog warning shown at prepare time.
	Warning string
	// CreatedAt is when the action was prepared.
	CreatedAt time.Time
}

// Store keeps at most one pending action with a TTL.
type Store struct {
	mu      sync.Mutex
	ttl     time.Duration
	now     func() time.Time
	pending *Action
}

// New builds a store with the given TTL and clock.
func New(ttl time.Duration, now func() time.Time) *Store {
	if now == nil {
		now = time.Now
	}
	return &Store{ttl: ttl, now: now}
}

// TTL returns the configured lifetime of a pending action.
func (s *Store) TTL() time.Duration { return s.ttl }

// Put replaces any previous pending action.
func (s *Store) Put(a Action) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if a.CreatedAt.IsZero() {
		a.CreatedAt = s.now()
	}
	stored := a
	s.pending = &stored
}

// Peek returns the pending action without consuming it. Expired actions are
// dropped and reported as absent.
func (s *Store) Peek() (Action, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	return s.peekLocked()
}

// Take returns and clears the pending action. Expired actions are dropped.
func (s *Store) Take() (Action, bool) {
	s.mu.Lock()
	defer s.mu.Unlock()
	a, ok := s.peekLocked()
	if !ok {
		return Action{}, false
	}
	s.pending = nil
	return a, true
}

// Clear drops any pending action.
func (s *Store) Clear() {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.pending = nil
}

func (s *Store) peekLocked() (Action, bool) {
	if s.pending == nil {
		return Action{}, false
	}
	if s.now().Sub(s.pending.CreatedAt) >= s.ttl {
		s.pending = nil
		return Action{}, false
	}
	return *s.pending, true
}
