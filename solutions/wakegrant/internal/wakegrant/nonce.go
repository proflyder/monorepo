package wakegrant

import (
	"crypto/rand"
	"encoding/base64"
	"errors"
	"sync"
	"time"
)

// NonceTTL is fixed by PROTOCOL.md: a challenge is valid for 60 s.
const NonceTTL = 60 * time.Second

var errNonce = errors.New("nonce is unknown, expired, reused or bound to another machine")

type nonceEntry struct {
	machine string
	expires time.Time
	used    bool
}

// nonceStore keeps single-use challenges in memory. A pod restart loses
// outstanding nonces, which is safe: the machine simply requests a new one;
// grants themselves live in SQLite.
type nonceStore struct {
	mu        sync.Mutex
	entries   map[string]*nonceEntry
	lastSweep time.Time
}

func newNonceStore() *nonceStore {
	return &nonceStore{entries: map[string]*nonceEntry{}}
}

// Issue creates a new 32-byte nonce bound to machine.
func (n *nonceStore) Issue(machine string, now time.Time) (string, error) {
	raw := make([]byte, 32)
	if _, err := rand.Read(raw); err != nil {
		return "", err
	}
	nonce := base64.StdEncoding.EncodeToString(raw)

	n.mu.Lock()
	defer n.mu.Unlock()
	n.sweepLocked(now)
	n.entries[nonce] = &nonceEntry{machine: machine, expires: now.Add(NonceTTL)}
	return nonce, nil
}

// Consume validates and atomically marks a nonce used. Marking happens before
// the caller verifies the MAC, exactly as PROTOCOL.md orders it, so a forged
// request burns its nonce and a replay always fails.
func (n *nonceStore) Consume(machine, nonce string, now time.Time) error {
	n.mu.Lock()
	defer n.mu.Unlock()
	entry, ok := n.entries[nonce]
	if !ok {
		return errNonce
	}
	if entry.used {
		return errNonce
	}
	if !now.Before(entry.expires) {
		delete(n.entries, nonce)
		return errNonce
	}
	if entry.machine != machine {
		return errNonce
	}
	entry.used = true
	return nil
}

// sweepLocked drops expired entries at most once a minute so an unauthenticated
// challenge flood cannot grow the map without bound.
func (n *nonceStore) sweepLocked(now time.Time) {
	if !n.lastSweep.IsZero() && now.Sub(n.lastSweep) < time.Minute {
		return
	}
	n.lastSweep = now
	for nonce, entry := range n.entries {
		if !now.Before(entry.expires) {
			delete(n.entries, nonce)
		}
	}
}

// Size reports the number of retained nonces (tests only).
func (n *nonceStore) Size() int {
	n.mu.Lock()
	defer n.mu.Unlock()
	return len(n.entries)
}
