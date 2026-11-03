package wakegrant

import (
	"sync"
	"time"
)

// ChallengeLimit is the protocol rate limit: 60 requests per minute per
// machine and per source IP, each bucket counted independently.
const (
	ChallengeLimit = 60
	RateWindow     = time.Minute
)

type rateEntry struct {
	start time.Time
	count int
}

// limiter is a small fixed-window counter. It is deliberately in-memory: a
// restart only resets the budget, which cannot be abused to bypass a grant.
type limiter struct {
	mu      sync.Mutex
	entries map[string]*rateEntry
	limit   int
	window  time.Duration
}

func newLimiter(limit int, window time.Duration) *limiter {
	return &limiter{entries: map[string]*rateEntry{}, limit: limit, window: window}
}

// allow reports whether key may perform one more request at now. Distinct
// prefixes keep the per-endpoint budgets separate ("challenge:", "claim:").
func (l *limiter) allow(key string, now time.Time) bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	entry, ok := l.entries[key]
	if !ok || now.Sub(entry.start) >= l.window {
		l.entries[key] = &rateEntry{start: now, count: 1}
		return true
	}
	if entry.count >= l.limit {
		return false
	}
	entry.count++
	return true
}

// prune drops stale windows; called opportunistically from the service.
func (l *limiter) prune(now time.Time) {
	l.mu.Lock()
	defer l.mu.Unlock()
	for key, entry := range l.entries {
		if now.Sub(entry.start) >= l.window {
			delete(l.entries, key)
		}
	}
}
