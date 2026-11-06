// Package profile reads the Home Assistant voice-profile entity and maps its
// state onto the routing profile.
//
// Two details are part of the contract:
//
//   - only a fresh *successful* read is cached; a failed read returns unknown
//     and clears the cache, so a stale profile is never used;
//   - the state is normalised with NFKC and a full Unicode case fold before
//     mapping, so fullwidth and non-breaking forms are recognised too.
package profile

import (
	"context"
	"net/http"
	"strings"
	"sync"
	"time"

	"golang.org/x/text/cases"
	"golang.org/x/text/unicode/norm"

	"github.com/proflyder/monorepo/packages/go/ha"
	"github.com/proflyder/monorepo/solutions/voice-proxy/internal/sanitize"
)

// Routing profiles.
const (
	Work     = "work"
	Personal = "personal"
)

// Config configures a Reader.
type Config struct {
	// URL is the Home Assistant base URL (no trailing slash).
	URL string
	// Token is resolved on every request, so a rotated mounted secret is picked
	// up without a restart.
	Token func() string
	// Entity is the profile input_select entity id.
	Entity string
	// Cache is HERMES_VOICE_PROFILE_CACHE_MS as a duration.
	Cache time.Duration
	// Timeout is HERMES_HA_TIMEOUT.
	Timeout time.Duration
	// HTTP is the shared HTTP client; nil builds one that does not follow
	// redirects and verifies TLS.
	HTTP *http.Client
	// Clock is time.Now in production and a fake clock in tests.
	Clock func() time.Time
}

// Reader reads HA entity states and caches only a successful profile read.
type Reader struct {
	url      string
	client   *ha.Client
	entity   string
	cache    time.Duration
	timeout  time.Duration
	clock    func() time.Time
	mu       sync.Mutex
	cached   string
	expireAt time.Time
}

// New builds a Reader.
func New(cfg Config) *Reader {
	timeout := cfg.Timeout
	if timeout <= 0 {
		timeout = 3 * time.Second
	}
	clock := cfg.Clock
	if clock == nil {
		clock = time.Now
	}
	return &Reader{
		url:     strings.TrimSuffix(strings.TrimSpace(cfg.URL), "/"),
		client:  ha.NewWithToken(cfg.URL, cfg.Token, cfg.HTTP),
		entity:  cfg.Entity,
		cache:   cfg.Cache,
		timeout: timeout,
		clock:   clock,
	}
}

// Profile returns the current routing profile. ok is false when the state is
// unknown, unavailable or unreadable. A fresh successful read is cached for the
// configured TTL; a failed read never is.
func (r *Reader) Profile(ctx context.Context) (string, bool) {
	r.mu.Lock()
	defer r.mu.Unlock()
	now := r.clock()
	if r.cached != "" && r.expireAt.After(now) {
		return r.cached, true
	}
	// A stale cache is never used: a read error makes the profile unknown.
	r.cached = ""
	r.expireAt = time.Time{}
	state, ok := r.state(ctx, r.entity)
	if !ok {
		return "", false
	}
	mapped, ok := MapState(state)
	if !ok {
		return "", false
	}
	r.cached = mapped
	r.expireAt = now.Add(r.cache)
	return mapped, true
}

// Fetch reads the profile without touching the cache. It is used by the
// updates poller and the queue consumer, which need a fresh value.
func (r *Reader) Fetch(ctx context.Context) (string, bool) {
	state, ok := r.state(ctx, r.entity)
	if !ok {
		return "", false
	}
	return MapState(state)
}

// State reads one entity state; ok is false for a transport error, a non-2xx
// response, a missing state or a body that is not a JSON object.
func (r *Reader) State(ctx context.Context, entity string) (string, bool) {
	return r.state(ctx, entity)
}

func (r *Reader) state(ctx context.Context, entity string) (string, bool) {
	if r.url == "" || entity == "" {
		return "", false
	}
	reqCtx, cancel := context.WithTimeout(ctx, r.timeout)
	defer cancel()
	state, err := r.client.State(reqCtx, entity)
	if err != nil {
		return "", false
	}
	return state, true
}

// MapState maps an input_select state onto a routing profile. Anything else is
// unknown.
func MapState(state string) (string, bool) {
	normalized := NormalizeState(state)
	switch normalized {
	case "work", "рабочий", "рабочий профиль":
		return Work, true
	case "personal", "личный", "личный профиль":
		return Personal, true
	}
	return "", false
}

// NormalizeState is NFKC, full Unicode case folding and whitespace collapsing.
func NormalizeState(state string) string {
	normalized := norm.NFKC.String(state)
	normalized = cases.Fold().String(normalized)
	return sanitize.CollapseSpace(normalized)
}
