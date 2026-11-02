// Package agentmode holds the in-memory "agent conversation" switch of the
// voice path.
//
// At most one bb thread is the active conversation. The state lives in the
// process on purpose: it is a short-lived routing hint, is bound to one
// gateway replica, and must disappear on restart rather than survive as stale
// configuration.
package agentmode

import "sync"

// State is the single active conversation thread.
type State struct {
	mu       sync.RWMutex
	threadID string
}

// New builds an empty state.
func New() *State { return &State{} }

// Enter makes threadID the active conversation.
func (s *State) Enter(threadID string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.threadID = threadID
}

// Exit clears the active conversation.
func (s *State) Exit() {
	s.mu.Lock()
	defer s.mu.Unlock()
	s.threadID = ""
}

// Active reports the active thread, if any.
func (s *State) Active() (string, bool) {
	s.mu.RLock()
	defer s.mu.RUnlock()
	return s.threadID, s.threadID != ""
}

// ClearIf clears the state only when it still points at threadID. It is used
// when a thread ends or is reported missing, so a newer conversation is not
// dropped by a stale failure.
func (s *State) ClearIf(threadID string) {
	s.mu.Lock()
	defer s.mu.Unlock()
	if s.threadID == threadID {
		s.threadID = ""
	}
}
