// Package profile reads the Home Assistant voice-profile entity and maps its
// state onto the routing profile.
//
// It is a port of read_entity_state(), profile_from_state(), read_profile()
// and _fetch_profile() in voice_proxy.py. Two details are part of the contract:
//
//   - only a fresh *successful* read is cached; a failed read returns unknown
//     and clears the cache, so a stale profile is never used;
//   - the state is normalised with NFKC and a full Unicode case fold before
//     mapping, exactly like Python's unicodedata.normalize + str.casefold().
package profile

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"strings"
	"sync"
	"time"

	"golang.org/x/text/cases"
	"golang.org/x/text/unicode/norm"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/sanitize"
)

// Routing profiles.
const (
	Work     = "work"
	Personal = "personal"
)

// maxStateBytes bounds one HA state response.
const maxStateBytes = 1 << 20

// Config configures a Reader.
type Config struct {
	// URL is the Home Assistant base URL (no trailing slash).
	URL string
	// Token is resolved on every request, so a rotated mounted secret is picked
	// up without a restart (ha_token() in voice_proxy.py).
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
	token    func() string
	entity   string
	cache    time.Duration
	timeout  time.Duration
	http     *http.Client
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
	hc := cfg.HTTP
	if hc == nil {
		hc = &http.Client{}
	}
	clock := cfg.Clock
	if clock == nil {
		clock = time.Now
	}
	token := cfg.Token
	if token == nil {
		token = func() string { return "" }
	}
	return &Reader{
		url:     strings.TrimSuffix(strings.TrimSpace(cfg.URL), "/"),
		token:   token,
		entity:  cfg.Entity,
		cache:   cfg.Cache,
		timeout: timeout,
		http:    hc,
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

// State reads one entity state; ok is false for a transport error, a non-object
// body, a missing state or a body that is not a JSON object.
func (r *Reader) State(ctx context.Context, entity string) (string, bool) {
	return r.state(ctx, entity)
}

func (r *Reader) state(ctx context.Context, entity string) (string, bool) {
	if r.url == "" || entity == "" {
		return "", false
	}
	reqCtx, cancel := context.WithTimeout(ctx, r.timeout)
	defer cancel()
	req, err := http.NewRequestWithContext(reqCtx, http.MethodGet, r.url+"/api/states/"+entity, nil)
	if err != nil {
		return "", false
	}
	req.Header.Set("Authorization", "Bearer "+r.token())
	resp, err := r.http.Do(req)
	if err != nil {
		return "", false
	}
	defer func() { _ = resp.Body.Close() }()
	if resp.StatusCode < 200 || resp.StatusCode >= 300 {
		_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, maxStateBytes))
		return "", false
	}
	var payload any
	if err := json.NewDecoder(io.LimitReader(resp.Body, maxStateBytes)).Decode(&payload); err != nil {
		return "", false
	}
	object, ok := payload.(map[string]any)
	if !ok {
		return "", false
	}
	state, ok := object["state"].(string)
	return state, ok
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

// NormalizeState is " ".join(NFKC(state).casefold().split()).
func NormalizeState(state string) string {
	normalized := norm.NFKC.String(state)
	normalized = cases.Fold().String(normalized)
	return sanitize.CollapseSpace(normalized)
}
