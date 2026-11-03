package wakegrant

import (
	"bytes"
	"context"
	"encoding/base64"
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"sync"
	"testing"
	"time"
)

type fakeClock struct {
	mu sync.Mutex
	t  time.Time
}

func (c *fakeClock) Now() time.Time {
	c.mu.Lock()
	defer c.mu.Unlock()
	return c.t
}

func (c *fakeClock) Advance(d time.Duration) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.t = c.t.Add(d)
}

type harness struct {
	t     *testing.T
	keys  Keys
	srv   *httptest.Server
	svc   *Service
	store *Store
	clock *fakeClock
	path  string
}

func newHarness(t *testing.T) *harness {
	t.Helper()
	dir := t.TempDir()
	path := filepath.Join(dir, "wakegrant.db")
	keys := Keys{
		"komp": vectorKey(),
		"desk": testKey(7),
	}
	clock := &fakeClock{t: time.Date(2026, 11, 3, 12, 0, 0, 0, time.UTC)}
	logger := slog.New(slog.NewTextHandler(io.Discard, nil))
	store, err := OpenStore(context.Background(), path)
	if err != nil {
		t.Fatalf("OpenStore: %v", err)
	}
	roleSvc := New(Options{
		Keys:        keys,
		IssuerToken: "test-issuer-token",
		Store:       store,
		MaxTTL:      MaxTTL,
		Logger:      logger,
		Now:         clock.Now,
	})
	srv := httptest.NewServer(roleSvc.Handler())
	h := &harness{t: t, keys: keys, srv: srv, svc: roleSvc, store: store, clock: clock, path: path}
	t.Cleanup(h.close)
	return h
}

func (h *harness) close() {
	if h.srv != nil {
		h.srv.Close()
		h.srv = nil
	}
	if h.store != nil {
		_ = h.store.Close()
		h.store = nil
	}
}

func (h *harness) key(machine string) []byte { return h.keys[machine] }

func (h *harness) post(path string, body any, bearer string) (int, []byte) {
	h.t.Helper()
	var reader io.Reader
	switch value := body.(type) {
	case string:
		reader = bytes.NewBufferString(value)
	case []byte:
		reader = bytes.NewReader(value)
	default:
		encoded, err := json.Marshal(value)
		if err != nil {
			h.t.Fatalf("marshal request: %v", err)
		}
		reader = bytes.NewReader(encoded)
	}
	req, err := http.NewRequest(http.MethodPost, h.srv.URL+path, reader)
	if err != nil {
		h.t.Fatalf("new request: %v", err)
	}
	req.Header.Set("Content-Type", "application/json")
	if bearer != "" {
		req.Header.Set("Authorization", "Bearer "+bearer)
	}
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		h.t.Fatalf("post %s: %v", path, err)
	}
	defer resp.Body.Close()
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		h.t.Fatalf("read %s: %v", path, err)
	}
	return resp.StatusCode, data
}

func (h *harness) issue(machine string, ttl *int) grantResponse {
	h.t.Helper()
	body := map[string]any{"machine": machine}
	if ttl != nil {
		body["ttlSeconds"] = *ttl
	}
	status, data := h.post("/v1/grants", body, "test-issuer-token")
	if status != http.StatusCreated {
		h.t.Fatalf("issue grant: status %d body %s", status, data)
	}
	var out grantResponse
	if err := json.Unmarshal(data, &out); err != nil {
		h.t.Fatalf("decode grant: %v", err)
	}
	return out
}

func (h *harness) challenge(machine string) string {
	h.t.Helper()
	status, data := h.post("/v1/challenge", map[string]string{"machine": machine}, "")
	if status != http.StatusOK {
		h.t.Fatalf("challenge: status %d body %s", status, data)
	}
	var out challengeResponse
	if err := json.Unmarshal(data, &out); err != nil {
		h.t.Fatalf("decode challenge: %v", err)
	}
	if out.ExpiresIn != 60 {
		h.t.Fatalf("expiresIn = %d, want 60", out.ExpiresIn)
	}
	return out.Nonce
}

func (h *harness) claim(machine, nonce string) (int, claimResponse, []byte) {
	h.t.Helper()
	mac := ClaimMAC(h.key(machine), machine, nonce)
	status, data := h.post("/v1/claim", map[string]string{
		"machine": machine, "nonce": nonce, "mac": mac,
	}, "")
	var out claimResponse
	_ = json.Unmarshal(data, &out)
	return status, out, data
}

func TestFullFlow(t *testing.T) {
	h := newHarness(t)
	grant := h.issue("komp", nil)
	if grant.GrantID == "" {
		t.Fatal("grantId is empty")
	}
	expires, err := time.Parse(time.RFC3339, grant.ExpiresAt)
	if err != nil {
		t.Fatalf("expiresAt %q: %v", grant.ExpiresAt, err)
	}
	if got := expires.Sub(h.clock.Now()); got != DefaultTTL {
		t.Fatalf("expiresAt - now = %s, want %s", got, DefaultTTL)
	}

	nonce := h.challenge("komp")
	status, claim, raw := h.claim("komp", nonce)
	if status != http.StatusOK {
		t.Fatalf("claim: status %d body %s", status, raw)
	}
	if !claim.Granted || claim.GrantID != grant.GrantID {
		t.Fatalf("claim = %+v, want granted grant %s", claim, grant.GrantID)
	}
	if want := GrantedMAC(h.key("komp"), "komp", nonce, grant.GrantID); claim.MAC != want {
		t.Fatalf("response mac = %q, want %q", claim.MAC, want)
	}
}

func TestGrantSurvivesRestart(t *testing.T) {
	h := newHarness(t)
	grant := h.issue("komp", nil)
	// Simulate a pod restart between WoL and claim: the process dies, the PVC
	// and the SQLite file stay.
	h.srv.Close()
	h.srv = nil
	_ = h.store.Close()
	h.store = nil

	store, err := OpenStore(context.Background(), h.path)
	if err != nil {
		t.Fatalf("reopen store: %v", err)
	}
	h.store = store
	h.svc = New(Options{
		Keys:        h.keys,
		IssuerToken: "test-issuer-token",
		Store:       store,
		MaxTTL:      MaxTTL,
		Logger:      slog.New(slog.NewTextHandler(io.Discard, nil)),
		Now:         h.clock.Now,
	})
	h.srv = httptest.NewServer(h.svc.Handler())
	t.Cleanup(func() { h.srv.Close() })

	nonce := h.challenge("komp")
	status, claim, raw := h.claim("komp", nonce)
	if status != http.StatusOK || !claim.Granted || claim.GrantID != grant.GrantID {
		t.Fatalf("claim after restart: status %d claim %+v body %s", status, claim, raw)
	}
}

func TestNonceIsSingleUse(t *testing.T) {
	h := newHarness(t)
	h.issue("komp", nil)
	nonce := h.challenge("komp")
	if status, claim, _ := h.claim("komp", nonce); status != http.StatusOK || !claim.Granted {
		t.Fatalf("first claim: status %d claim %+v", status, claim)
	}
	status, _, raw := h.claim("komp", nonce)
	if status != http.StatusUnauthorized {
		t.Fatalf("replayed nonce: status %d body %s, want 401", status, raw)
	}
}

func TestWrongMachineCannotUseNonce(t *testing.T) {
	h := newHarness(t)
	h.issue("komp", nil)
	nonce := h.challenge("komp")
	status, _, raw := h.claim("desk", nonce)
	if status != http.StatusUnauthorized {
		t.Fatalf("wrong machine: status %d body %s, want 401", status, raw)
	}
	// The mismatched attempt must not burn the nonce.
	status, claim, raw := h.claim("komp", nonce)
	if status != http.StatusOK || !claim.Granted {
		t.Fatalf("owner claim after wrong machine: status %d claim %+v body %s", status, claim, raw)
	}
}

func TestBadMACBurnsTheNonce(t *testing.T) {
	h := newHarness(t)
	h.issue("komp", nil)
	nonce := h.challenge("komp")
	status, data := h.post("/v1/claim", map[string]string{
		"machine": "komp", "nonce": nonce,
		"mac": base64.StdEncoding.EncodeToString(make([]byte, 32)),
	}, "")
	if status != http.StatusUnauthorized {
		t.Fatalf("bad mac: status %d body %s, want 401", status, data)
	}
	if status, _, _ := h.claim("komp", nonce); status != http.StatusUnauthorized {
		t.Fatalf("nonce after bad mac: status %d, want 401", status)
	}
}

func TestExpiredGrantIsDenied(t *testing.T) {
	h := newHarness(t)
	h.issue("komp", ptr(30))
	h.clock.Advance(31 * time.Second)
	nonce := h.challenge("komp")
	status, claim, raw := h.claim("komp", nonce)
	if status != http.StatusOK {
		t.Fatalf("claim expired: status %d body %s", status, raw)
	}
	if claim.Granted {
		t.Fatalf("expired grant was granted: %+v", claim)
	}
	if want := DeniedMAC(h.key("komp"), "komp", nonce); claim.MAC != want {
		t.Fatalf("denied mac = %q, want %q", claim.MAC, want)
	}
}

func TestExpiredNonceIsRejected(t *testing.T) {
	h := newHarness(t)
	h.issue("komp", ptr(600))
	nonce := h.challenge("komp")
	h.clock.Advance(61 * time.Second)
	status, _, raw := h.claim("komp", nonce)
	if status != http.StatusUnauthorized {
		t.Fatalf("expired nonce: status %d body %s, want 401", status, raw)
	}
}

func TestDoubleClaimOnlyOneWins(t *testing.T) {
	h := newHarness(t)
	h.issue("komp", nil)
	nonceA := h.challenge("komp")
	nonceB := h.challenge("komp")

	type result struct {
		status int
		claim  claimResponse
		data   []byte
	}
	run := func(nonce string) result {
		mac := ClaimMAC(h.key("komp"), "komp", nonce)
		body, _ := json.Marshal(map[string]string{"machine": "komp", "nonce": nonce, "mac": mac})
		req, _ := http.NewRequest(http.MethodPost, h.srv.URL+"/v1/claim", bytes.NewReader(body))
		resp, err := http.DefaultClient.Do(req)
		if err != nil {
			t.Errorf("claim: %v", err)
			return result{}
		}
		defer resp.Body.Close()
		data, _ := io.ReadAll(resp.Body)
		var out claimResponse
		_ = json.Unmarshal(data, &out)
		return result{status: resp.StatusCode, claim: out, data: data}
	}

	start := make(chan struct{})
	results := make([]result, 2)
	var wg sync.WaitGroup
	for i, nonce := range []string{nonceA, nonceB} {
		wg.Add(1)
		go func(i int, nonce string) {
			defer wg.Done()
			<-start
			results[i] = run(nonce)
		}(i, nonce)
	}
	close(start)
	wg.Wait()

	granted, denied := 0, 0
	for _, result := range results {
		if result.status != http.StatusOK {
			t.Fatalf("claim status %d body %s", result.status, result.data)
		}
		if result.claim.Granted {
			granted++
		} else {
			denied++
		}
	}
	if granted != 1 || denied != 1 {
		t.Fatalf("granted=%d denied=%d, want exactly one of each", granted, denied)
	}
}

func TestNewestGrantWins(t *testing.T) {
	h := newHarness(t)
	first := h.issue("komp", nil)
	second := h.issue("komp", nil)
	if first.GrantID == second.GrantID {
		t.Fatal("grant ids must differ")
	}
	nonce := h.challenge("komp")
	_, claim, raw := h.claim("komp", nonce)
	if !claim.Granted || claim.GrantID != second.GrantID {
		t.Fatalf("claim = %+v body %s, want newest grant %s", claim, raw, second.GrantID)
	}
}

func TestIssuerRequiresTheBearerToken(t *testing.T) {
	h := newHarness(t)
	body := map[string]string{"machine": "komp"}
	for name, bearer := range map[string]string{
		"missing": "",
		"wrong":   "nope",
		"prefix":  "Basic test-issuer-token",
	} {
		status, data := h.post("/v1/grants", body, bearer)
		if status != http.StatusUnauthorized {
			t.Errorf("%s: status %d body %s, want 401", name, status, data)
		}
	}
}

func TestUnknownMachineIsRejected(t *testing.T) {
	h := newHarness(t)
	status, data := h.post("/v1/grants", map[string]string{"machine": "ghost"}, "test-issuer-token")
	if status != http.StatusBadRequest {
		t.Errorf("unknown machine grant: status %d body %s, want 400", status, data)
	}
	status, data = h.post("/v1/challenge", map[string]string{"machine": "ghost"}, "")
	if status != http.StatusBadRequest {
		t.Errorf("unknown machine challenge: status %d body %s, want 400", status, data)
	}
	status, data = h.post("/v1/claim", map[string]string{"machine": "ghost", "nonce": "x", "mac": "y"}, "")
	if status != http.StatusUnauthorized {
		t.Errorf("unknown machine claim: status %d body %s, want 401", status, data)
	}
}

func TestTTLBounds(t *testing.T) {
	h := newHarness(t)
	for _, ttl := range []int{29, 601, 0, -1} {
		status, data := h.post("/v1/grants", map[string]any{
			"machine": "komp", "ttlSeconds": ttl,
		}, "test-issuer-token")
		if status != http.StatusBadRequest {
			t.Errorf("ttl %d: status %d body %s, want 400", ttl, status, data)
		}
	}
	grant := h.issue("komp", ptr(600))
	expires, err := time.Parse(time.RFC3339, grant.ExpiresAt)
	if err != nil {
		t.Fatal(err)
	}
	if got := expires.Sub(h.clock.Now()); got != 600*time.Second {
		t.Fatalf("expiresAt - now = %s, want 600s", got)
	}
}

func TestMalformedJSON(t *testing.T) {
	h := newHarness(t)
	for _, endpoint := range []string{"/v1/grants", "/v1/challenge", "/v1/claim"} {
		for name, body := range map[string]string{
			"broken":   `{"machine":`,
			"wrong":    `{"machine":123}`,
			"trailing": `{"machine":"komp"} {"machine":"komp"}`,
			"empty":    ``,
		} {
			bearer := ""
			if endpoint == "/v1/grants" {
				bearer = "test-issuer-token"
			}
			status, data := h.post(endpoint, body, bearer)
			if status != http.StatusBadRequest {
				t.Errorf("%s %s: status %d body %s, want 400", endpoint, name, status, data)
			}
		}
	}
}

func TestChallengeRateLimit(t *testing.T) {
	h := newHarness(t)
	for i := 0; i < ChallengeLimit; i++ {
		if status, data := h.post("/v1/challenge", map[string]string{"machine": "komp"}, ""); status != http.StatusOK {
			t.Fatalf("challenge %d: status %d body %s", i, status, data)
		}
	}
	status, data := h.post("/v1/challenge", map[string]string{"machine": "komp"}, "")
	if status != http.StatusTooManyRequests {
		t.Fatalf("challenge over limit: status %d body %s, want 429", status, data)
	}
	// Another machine has its own per-machine bucket (the test IP bucket is
	// shared, so only assert the machine bucket by advancing the window).
	h.clock.Advance(61 * time.Second)
	if status, data := h.post("/v1/challenge", map[string]string{"machine": "desk"}, ""); status != http.StatusOK {
		t.Fatalf("other machine after window: status %d body %s", status, data)
	}
}

func TestHealthz(t *testing.T) {
	h := newHarness(t)
	resp, err := http.Get(h.srv.URL + "/healthz")
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		t.Fatalf("healthz status %d", resp.StatusCode)
	}
}

func ptr(v int) *int { return &v }
