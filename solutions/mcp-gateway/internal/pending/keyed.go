// Keyed is the multi-key variant of Store.
//
// Infra keeps its single rollout-restart action in Store. Other tool groups
// (bb.read/bb.act, future domains) keep their own pending values in the same
// process without ever colliding with each other: every entry lives under an
// explicit key and expires on its own schedule.
package pending

import (
	"sync"
	"time"
)

type keyedEntry[T any] struct {
	value     T
	createdAt time.Time
}

// Keyed stores at most one value per key with a shared TTL.
type Keyed[T any] struct {
	mu      sync.Mutex
	ttl     time.Duration
	now     func() time.Time
	entries map[string]keyedEntry[T]
}

// NewKeyed builds a keyed store with the given TTL and clock.
func NewKeyed[T any](ttl time.Duration, now func() time.Time) *Keyed[T] {
	if now == nil {
		now = time.Now
	}
	return &Keyed[T]{ttl: ttl, now: now, entries: make(map[string]keyedEntry[T])}
}

// TTL returns the configured lifetime of one entry.
func (k *Keyed[T]) TTL() time.Duration { return k.ttl }

// Put replaces any previous value under key and returns its creation time.
func (k *Keyed[T]) Put(key string, value T) time.Time {
	k.mu.Lock()
	defer k.mu.Unlock()
	createdAt := k.now()
	k.entries[key] = keyedEntry[T]{value: value, createdAt: createdAt}
	return createdAt
}

// Peek returns the value under key without consuming it. Expired values are
// dropped and reported as absent.
func (k *Keyed[T]) Peek(key string) (T, bool) {
	k.mu.Lock()
	defer k.mu.Unlock()
	return k.peekLocked(key)
}

// Take returns and clears the value under key. Expired values are dropped.
func (k *Keyed[T]) Take(key string) (T, bool) {
	k.mu.Lock()
	defer k.mu.Unlock()
	value, ok := k.peekLocked(key)
	if !ok {
		return value, false
	}
	delete(k.entries, key)
	return value, true
}

// Clear drops the value under key.
func (k *Keyed[T]) Clear(key string) {
	k.mu.Lock()
	defer k.mu.Unlock()
	delete(k.entries, key)
}

func (k *Keyed[T]) peekLocked(key string) (T, bool) {
	var zero T
	entry, ok := k.entries[key]
	if !ok {
		return zero, false
	}
	if k.now().Sub(entry.createdAt) >= k.ttl {
		delete(k.entries, key)
		return zero, false
	}
	return entry.value, true
}
