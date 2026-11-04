package wakegrant

import (
	"bytes"
	"context"
	"database/sql"
	"encoding/base64"
	"encoding/json"
	"errors"
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

func (h *harness) claimPurpose(machine, nonce, purpose string, wait *int) (int, claimResponse, []byte) {
	h.t.Helper()
	out := h.claimRaw(machine, nonce, purpose, wait)
	return out.status, out.claim, out.raw
}

func (h *harness) issuePurpose(machine, purpose string, ttl *int) grantResponse {
	h.t.Helper()
	body := map[string]any{"machine": machine, "purpose": purpose}
	if ttl != nil {
		body["ttlSeconds"] = *ttl
	}
	status, data := h.post("/v1/grants", body, "test-issuer-token")
	if status != http.StatusCreated {
		h.t.Fatalf("issue %s grant: status %d body %s", purpose, status, data)
	}
	var out grantResponse
	if err := json.Unmarshal(data, &out); err != nil {
		h.t.Fatalf("decode grant: %v", err)
	}
	return out
}

type claimResult struct {
	status int
	claim  claimResponse
	raw    []byte
	err    error
}

// claimRaw is goroutine-safe: it never calls t.Fatalf.
func (h *harness) claimRaw(machine, nonce, purpose string, waitSeconds *int) claimResult {
	h.t.Helper()
	key, ok := h.keys[machine]
	if !ok {
		return claimResult{err: errUnknownTestMachine}
	}
	body := map[string]any{
		"machine": machine,
		"nonce":   nonce,
		"purpose": purpose,
	}
	if purpose != "" {
		body["mac"] = ClaimMACForPurpose(key, machine, nonce, Purpose(purpose))
	} else {
		body["mac"] = ClaimMAC(key, machine, nonce)
	}
	if waitSeconds != nil {
		body["waitSeconds"] = *waitSeconds
	}
	encoded, err := json.Marshal(body)
	if err != nil {
		return claimResult{err: err}
	}
	req, err := http.NewRequest(http.MethodPost, h.srv.URL+"/v1/claim", bytes.NewReader(encoded))
	if err != nil {
		return claimResult{err: err}
	}
	req.Header.Set("Content-Type", "application/json")
	resp, err := http.DefaultClient.Do(req)
	if err != nil {
		return claimResult{err: err}
	}
	defer resp.Body.Close()
	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return claimResult{err: err, status: resp.StatusCode}
	}
	var out claimResponse
	_ = json.Unmarshal(data, &out)
	return claimResult{status: resp.StatusCode, claim: out, raw: data}
}

var errUnknownTestMachine = errors.New("unknown test machine")

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

func TestPurposeGrantsReplaceWithinTheirPurposeOnly(t *testing.T) {
	h := newHarness(t)
	firstLogon := h.issuePurpose("komp", "logon", nil)
	unlock := h.issuePurpose("komp", "unlock", nil)
	secondLogon := h.issuePurpose("komp", "logon", nil)
	if firstLogon.GrantID == secondLogon.GrantID {
		t.Fatal("grant ids must differ")
	}

	status, claim, raw := h.claimPurpose("komp", h.challenge("komp"), "unlock", nil)
	if status != http.StatusOK || !claim.Granted || claim.GrantID != unlock.GrantID {
		t.Fatalf("unlock claim: status %d claim %+v body %s", status, claim, raw)
	}
	status, claim, raw = h.claimPurpose("komp", h.challenge("komp"), "logon", nil)
	if status != http.StatusOK || !claim.Granted || claim.GrantID != secondLogon.GrantID {
		t.Fatalf("logon claim: status %d claim %+v body %s", status, claim, raw)
	}
}

func TestLogonGrantIsNotOfferedToAnUnlockClaim(t *testing.T) {
	h := newHarness(t)
	logon := h.issuePurpose("komp", "logon", nil)

	nonce := h.challenge("komp")
	status, claim, raw := h.claimPurpose("komp", nonce, "unlock", nil)
	if status != http.StatusOK {
		t.Fatalf("unlock claim: status %d body %s", status, raw)
	}
	if claim.Granted {
		t.Fatalf("logon grant was granted to an unlock claim: %+v", claim)
	}
	if want := DeniedMACForPurpose(h.key("komp"), "komp", nonce, PurposeUnlock); claim.MAC != want {
		t.Fatalf("denied mac = %q, want %q", claim.MAC, want)
	}

	// The logon grant is still there for the right purpose.
	status, claim, raw = h.claimPurpose("komp", h.challenge("komp"), "logon", nil)
	if status != http.StatusOK || !claim.Granted || claim.GrantID != logon.GrantID {
		t.Fatalf("logon claim after unlock miss: status %d claim %+v body %s", status, claim, raw)
	}
}

func TestUnlockGrantIsNotOfferedToALogonClaim(t *testing.T) {
	h := newHarness(t)
	unlock := h.issuePurpose("komp", "unlock", nil)

	status, claim, raw := h.claimPurpose("komp", h.challenge("komp"), "logon", nil)
	if status != http.StatusOK || claim.Granted {
		t.Fatalf("logon claim got an unlock grant: status %d claim %+v body %s", status, claim, raw)
	}
	status, claim, raw = h.claimPurpose("komp", h.challenge("komp"), "unlock", nil)
	if status != http.StatusOK || !claim.Granted || claim.GrantID != unlock.GrantID {
		t.Fatalf("unlock claim: status %d claim %+v body %s", status, claim, raw)
	}
}

func TestClaimWithoutPurposeKeepsV1Messages(t *testing.T) {
	h := newHarness(t)
	// A v1-style grant (no purpose field) defaults to logon.
	grant := h.issue("komp", nil)

	nonce := h.challenge("komp")
	mac := ClaimMAC(h.key("komp"), "komp", nonce)
	status, data := h.post("/v1/claim", map[string]string{
		"machine": "komp", "nonce": nonce, "mac": mac,
	}, "")
	if status != http.StatusOK {
		t.Fatalf("v1 claim: status %d body %s", status, data)
	}
	var claim claimResponse
	if err := json.Unmarshal(data, &claim); err != nil {
		t.Fatal(err)
	}
	if !claim.Granted || claim.GrantID != grant.GrantID {
		t.Fatalf("v1 claim = %+v, want grant %s", claim, grant.GrantID)
	}
	if want := GrantedMAC(h.key("komp"), "komp", nonce, grant.GrantID); claim.MAC != want {
		t.Fatalf("v1 granted mac = %q, want %q", claim.MAC, want)
	}

	// A v1 denial also keeps the old bytes: no purpose line.
	nonce = h.challenge("komp")
	mac = ClaimMAC(h.key("komp"), "komp", nonce)
	status, data = h.post("/v1/claim", map[string]string{
		"machine": "komp", "nonce": nonce, "mac": mac,
	}, "")
	if status != http.StatusOK {
		t.Fatalf("v1 denied claim: status %d body %s", status, data)
	}
	if err := json.Unmarshal(data, &claim); err != nil {
		t.Fatal(err)
	}
	if claim.Granted {
		t.Fatal("a v1 claim must not consume an unlock grant")
	}
	if want := DeniedMAC(h.key("komp"), "komp", nonce); claim.MAC != want {
		t.Fatalf("v1 denied mac = %q, want %q", claim.MAC, want)
	}
}

func TestLongPollReceivesGrantIssuedWhileWaiting(t *testing.T) {
	h := newHarness(t)
	nonce := h.challenge("komp")

	type outcome struct{ result claimResult }
	done := make(chan outcome, 1)
	started := make(chan struct{})
	go func() {
		close(started)
		done <- outcome{h.claimRaw("komp", nonce, "unlock", ptr(3))}
	}()
	<-started
	time.Sleep(150 * time.Millisecond)

	grant := h.issuePurpose("komp", "unlock", nil)

	select {
	case out := <-done:
		if out.result.err != nil {
			t.Fatalf("long-poll claim: %v", out.result.err)
		}
		if out.result.status != http.StatusOK || !out.result.claim.Granted {
			t.Fatalf("long-poll result: status %d claim %+v", out.result.status, out.result.claim)
		}
		if out.result.claim.GrantID != grant.GrantID {
			t.Fatalf("granted %s, want %s", out.result.claim.GrantID, grant.GrantID)
		}
		if want := GrantedMACForPurpose(h.key("komp"), "komp", nonce, grant.GrantID, PurposeUnlock); out.result.claim.MAC != want {
			t.Fatalf("long-poll mac = %q, want %q", out.result.claim.MAC, want)
		}
	case <-time.After(3 * time.Second):
		t.Fatal("long-poll did not return after a grant was issued")
	}
}

func TestLongPollTimesOutWithADeniedClaim(t *testing.T) {
	h := newHarness(t)
	nonce := h.challenge("komp")
	start := time.Now()
	out := h.claimRaw("komp", nonce, "unlock", ptr(1))
	elapsed := time.Since(start)
	if out.err != nil {
		t.Fatalf("long-poll claim: %v", out.err)
	}
	if out.status != http.StatusOK || out.claim.Granted {
		t.Fatalf("timeout result: status %d claim %+v", out.status, out.claim)
	}
	if elapsed < 900*time.Millisecond {
		t.Fatalf("long-poll returned after %s, want about 1s", elapsed)
	}
	if want := DeniedMACForPurpose(h.key("komp"), "komp", nonce, PurposeUnlock); out.claim.MAC != want {
		t.Fatalf("timeout mac = %q, want %q", out.claim.MAC, want)
	}
}

func TestWaitersPerMachineAreLimited(t *testing.T) {
	h := newHarness(t)
	// Register the maximum number of waits directly so the cap is tested
	// deterministically, then prove the HTTP path maps it to 429.
	for i := 0; i < MaxWaitersPerQueue; i++ {
		_, release, ok := h.svc.waiters.acquire("komp", PurposeUnlock)
		if !ok {
			t.Fatalf("waiter %d was refused below the limit", i)
		}
		defer release()
	}
	out := h.claimRaw("komp", h.challenge("komp"), "unlock", ptr(1))
	if out.err != nil {
		t.Fatalf("long-poll claim: %v", out.err)
	}
	if out.status != http.StatusTooManyRequests {
		t.Fatalf("third waiter: status %d body %s, want 429", out.status, out.raw)
	}
	// The per-machine cap applies across purposes: a logon waiter is refused
	// while two waits are already outstanding.
	if _, _, ok := h.svc.waiters.acquire("komp", PurposeLogon); ok {
		t.Fatal("per-machine waiter cap must include every purpose")
	}
}

func TestWaitSecondsRequiresAValidPurpose(t *testing.T) {
	h := newHarness(t)
	for name, body := range map[string]map[string]any{
		"wait without purpose": {"machine": "komp", "nonce": "n", "mac": "m", "waitSeconds": 5},
		"wait zero":            {"machine": "komp", "nonce": "n", "mac": "m", "purpose": "unlock", "waitSeconds": 0},
		"wait too long":        {"machine": "komp", "nonce": "n", "mac": "m", "purpose": "unlock", "waitSeconds": 26},
	} {
		status, data := h.post("/v1/claim", body, "")
		if status != http.StatusBadRequest {
			t.Errorf("%s: status %d body %s, want 400", name, status, data)
		}
	}
}

func TestInvalidPurposeIsRejected(t *testing.T) {
	h := newHarness(t)
	status, data := h.post("/v1/grants", map[string]any{
		"machine": "komp", "purpose": "admin",
	}, "test-issuer-token")
	if status != http.StatusBadRequest {
		t.Errorf("grant bad purpose: status %d body %s, want 400", status, data)
	}
	status, data = h.post("/v1/claim", map[string]any{
		"machine": "komp", "nonce": "n", "mac": "m", "purpose": "admin",
	}, "")
	if status != http.StatusBadRequest {
		t.Errorf("claim bad purpose: status %d body %s, want 400", status, data)
	}
}

func TestMigrationDefaultsExistingGrantsToLogon(t *testing.T) {
	path := filepath.Join(t.TempDir(), "legacy.db")
	db, err := sql.Open("sqlite", path)
	if err != nil {
		t.Fatalf("open legacy db: %v", err)
	}
	legacy := []string{
		`CREATE TABLE schema_meta (version INTEGER NOT NULL)`,
		`INSERT INTO schema_meta(version) VALUES (1)`,
		`CREATE TABLE grants (
			id TEXT PRIMARY KEY,
			machine TEXT NOT NULL,
			created_unix INTEGER NOT NULL,
			expires_unix INTEGER NOT NULL,
			consumed INTEGER NOT NULL DEFAULT 0,
			consumed_unix INTEGER NOT NULL DEFAULT 0
		)`,
		`CREATE INDEX grants_machine ON grants(machine, consumed, expires_unix)`,
	}
	for _, statement := range legacy {
		if _, err := db.Exec(statement); err != nil {
			t.Fatalf("legacy schema: %v", err)
		}
	}
	now := time.Now()
	if _, err := db.Exec(`INSERT INTO grants(id, machine, created_unix, expires_unix, consumed) VALUES (?, ?, ?, ?, 0)`,
		"legacy-grant", "komp", now.UnixNano(), now.Add(time.Hour).Unix()); err != nil {
		t.Fatalf("insert legacy grant: %v", err)
	}
	if err := db.Close(); err != nil {
		t.Fatalf("close legacy db: %v", err)
	}

	store, err := OpenStore(context.Background(), path)
	if err != nil {
		t.Fatalf("migrate legacy db: %v", err)
	}
	defer store.Close()
	svc := New(Options{
		Keys:        Keys{"komp": vectorKey()},
		IssuerToken: "test-issuer-token",
		Store:       store,
		MaxTTL:      MaxTTL,
		Logger:      slog.New(slog.NewTextHandler(io.Discard, nil)),
	})
	srv := httptest.NewServer(svc.Handler())
	defer srv.Close()

	challenge, err := http.Post(srv.URL+"/v1/challenge", "application/json", bytes.NewBufferString(`{"machine":"komp"}`))
	if err != nil {
		t.Fatal(err)
	}
	var chal challengeResponse
	if err := json.NewDecoder(challenge.Body).Decode(&chal); err != nil {
		t.Fatal(err)
	}
	_ = challenge.Body.Close()
	mac := ClaimMAC(vectorKey(), "komp", chal.Nonce)
	body, _ := json.Marshal(map[string]string{"machine": "komp", "nonce": chal.Nonce, "mac": mac})
	resp, err := http.Post(srv.URL+"/v1/claim", "application/json", bytes.NewReader(body))
	if err != nil {
		t.Fatal(err)
	}
	defer resp.Body.Close()
	var claim claimResponse
	if err := json.NewDecoder(resp.Body).Decode(&claim); err != nil {
		t.Fatal(err)
	}
	if !claim.Granted || claim.GrantID != "legacy-grant" {
		t.Fatalf("legacy grant not claimable as logon: %+v", claim)
	}
}
