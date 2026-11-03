// Package wakegrant implements the wakegrant v1 protocol from
// packages/contracts/wakegrant/PROTOCOL.md: one-time logon grants for a
// machine woken by Home Assistant.
package wakegrant

import (
	"context"
	"crypto/hmac"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net"
	"net/http"
	"strings"
	"time"
)

const (
	maxBodyBytes = 4 << 10
	claimPrefix  = "claim:"
	chalPrefix   = "challenge:"
)

// Protocol TTL bounds. config has its own copy so that package loads no
// protocol code; both are pinned to PROTOCOL.md.
const (
	MinTTL     = 30 * time.Second
	MaxTTL     = 600 * time.Second
	DefaultTTL = 180 * time.Second
)

// Options configures the HTTP service.
type Options struct {
	Keys        Keys
	IssuerToken string
	Store       *Store
	MaxTTL      time.Duration
	Logger      *slog.Logger
	// Now is injectable for tests.
	Now func() time.Time
}

// Service is the wakegrant HTTP handler.
type Service struct {
	keys    Keys
	token   string
	store   *Store
	maxTTL  time.Duration
	logger  *slog.Logger
	now     func() time.Time
	nonces  *nonceStore
	limiter *limiter
}

// New builds a service. The caller owns the store lifecycle.
func New(opts Options) *Service {
	maxTTL := opts.MaxTTL
	if maxTTL <= 0 {
		maxTTL = 600 * time.Second
	}
	logger := opts.Logger
	if logger == nil {
		logger = slog.Default()
	}
	now := opts.Now
	if now == nil {
		now = time.Now
	}
	return &Service{
		keys:    opts.Keys,
		token:   opts.IssuerToken,
		store:   opts.Store,
		maxTTL:  maxTTL,
		logger:  logger,
		now:     now,
		nonces:  newNonceStore(),
		limiter: newLimiter(ChallengeLimit, RateWindow),
	}
}

// Handler returns the full HTTP handler (routes plus health).
func (s *Service) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("POST /v1/grants", s.handleGrant)
	mux.HandleFunc("POST /v1/challenge", s.handleChallenge)
	mux.HandleFunc("POST /v1/claim", s.handleClaim)
	mux.HandleFunc("GET /healthz", s.handleHealth)
	return mux
}

type grantRequest struct {
	Machine    string `json:"machine"`
	TTLSeconds *int   `json:"ttlSeconds"`
}

type grantResponse struct {
	GrantID   string `json:"grantId"`
	ExpiresAt string `json:"expiresAt"`
}

type challengeRequest struct {
	Machine string `json:"machine"`
}

type challengeResponse struct {
	Nonce     string `json:"nonce"`
	ExpiresIn int    `json:"expiresIn"`
}

type claimRequest struct {
	Machine string `json:"machine"`
	Nonce   string `json:"nonce"`
	MAC     string `json:"mac"`
}

type claimResponse struct {
	Granted bool   `json:"granted"`
	GrantID string `json:"grantId,omitempty"`
	MAC     string `json:"mac"`
}

func (s *Service) handleGrant(w http.ResponseWriter, r *http.Request) {
	now := s.now()
	if !s.authorized(r) {
		s.logOutcome("grant", "", "unauthorized", "")
		writeError(w, http.StatusUnauthorized)
		return
	}
	var req grantRequest
	if err := decodeJSON(w, r, &req); err != nil {
		s.logOutcome("grant", "", "bad-request", "")
		writeError(w, http.StatusBadRequest)
		return
	}
	if !MachinePattern.MatchString(req.Machine) {
		s.logOutcome("grant", "", "unknown-machine", "")
		writeError(w, http.StatusBadRequest)
		return
	}
	if _, ok := s.keys[req.Machine]; !ok {
		s.logOutcome("grant", req.Machine, "unknown-machine", "")
		writeError(w, http.StatusBadRequest)
		return
	}
	ttl := DefaultTTL
	if req.TTLSeconds != nil {
		ttl = time.Duration(*req.TTLSeconds) * time.Second
	}
	if ttl < MinTTL || ttl > MaxTTL || ttl > s.maxTTL {
		s.logOutcome("grant", req.Machine, "bad-ttl", "")
		writeError(w, http.StatusBadRequest)
		return
	}
	id, expires, err := s.store.CreateGrant(r.Context(), req.Machine, now, ttl)
	if err != nil {
		s.logOutcome("grant", req.Machine, "storage-error", "")
		writeError(w, http.StatusInternalServerError)
		return
	}
	s.logOutcome("grant", req.Machine, "issued", id)
	writeJSON(w, http.StatusCreated, grantResponse{
		GrantID:   id,
		ExpiresAt: expires.Format(time.RFC3339),
	})
}

func (s *Service) handleChallenge(w http.ResponseWriter, r *http.Request) {
	now := s.now()
	var req challengeRequest
	if err := decodeJSON(w, r, &req); err != nil {
		s.logOutcome("challenge", "", "bad-request", "")
		writeError(w, http.StatusBadRequest)
		return
	}
	if !MachinePattern.MatchString(req.Machine) {
		s.logOutcome("challenge", "", "unknown-machine", "")
		writeError(w, http.StatusBadRequest)
		return
	}
	if _, ok := s.keys[req.Machine]; !ok {
		s.logOutcome("challenge", req.Machine, "unknown-machine", "")
		writeError(w, http.StatusBadRequest)
		return
	}
	if !s.limit(chalPrefix, req.Machine, r, now) {
		s.logOutcome("challenge", req.Machine, "rate-limited", "")
		writeError(w, http.StatusTooManyRequests)
		return
	}
	nonce, err := s.nonces.Issue(req.Machine, now)
	if err != nil {
		s.logOutcome("challenge", req.Machine, "internal-error", "")
		writeError(w, http.StatusInternalServerError)
		return
	}
	s.logOutcome("challenge", req.Machine, "issued", "")
	writeJSON(w, http.StatusOK, challengeResponse{
		Nonce:     nonce,
		ExpiresIn: int(NonceTTL / time.Second),
	})
}

func (s *Service) handleClaim(w http.ResponseWriter, r *http.Request) {
	now := s.now()
	var req claimRequest
	if err := decodeJSON(w, r, &req); err != nil {
		s.logOutcome("claim", "", "bad-request", "")
		writeError(w, http.StatusBadRequest)
		return
	}
	if !MachinePattern.MatchString(req.Machine) || req.Nonce == "" || req.MAC == "" {
		s.logOutcome("claim", "", "bad-request", "")
		writeError(w, http.StatusBadRequest)
		return
	}
	key, ok := s.keys[req.Machine]
	if !ok {
		// Never reveal whether the machine exists.
		s.logOutcome("claim", "", "unauthorized", "")
		writeError(w, http.StatusUnauthorized)
		return
	}
	if !s.limit(claimPrefix, req.Machine, r, now) {
		s.logOutcome("claim", req.Machine, "rate-limited", "")
		writeError(w, http.StatusTooManyRequests)
		return
	}

	// Order matters (PROTOCOL.md): the nonce is consumed first, so a forged
	// MAC cannot be retried with the same nonce.
	if err := s.nonces.Consume(req.Machine, req.Nonce, now); err != nil {
		s.logOutcome("claim", req.Machine, "bad-nonce", "")
		writeError(w, http.StatusUnauthorized)
		return
	}
	if !verifyMAC([]byte(ClaimMAC(key, req.Machine, req.Nonce)), []byte(req.MAC)) {
		s.logOutcome("claim", req.Machine, "bad-mac", "")
		writeError(w, http.StatusUnauthorized)
		return
	}

	grantID, granted, err := s.store.ConsumeNewest(r.Context(), req.Machine, now)
	if err != nil {
		// The nonce is already burnt; fail closed without a MAC.
		s.logOutcome("claim", req.Machine, "storage-error", "")
		writeError(w, http.StatusInternalServerError)
		return
	}
	if granted {
		s.logOutcome("claim", req.Machine, "granted", grantID)
		writeJSON(w, http.StatusOK, claimResponse{
			Granted: true,
			GrantID: grantID,
			MAC:     GrantedMAC(key, req.Machine, req.Nonce, grantID),
		})
		return
	}
	s.logOutcome("claim", req.Machine, "denied", "")
	writeJSON(w, http.StatusOK, claimResponse{
		Granted: false,
		MAC:     DeniedMAC(key, req.Machine, req.Nonce),
	})
}

func (s *Service) handleHealth(w http.ResponseWriter, r *http.Request) {
	ctx, cancel := context.WithTimeout(r.Context(), 2*time.Second)
	defer cancel()
	if s.store == nil || s.store.Ping(ctx) != nil {
		w.Header().Set("Content-Type", "text/plain; charset=utf-8")
		w.WriteHeader(http.StatusServiceUnavailable)
		_, _ = io.WriteString(w, "unavailable\n")
		return
	}
	w.Header().Set("Content-Type", "text/plain; charset=utf-8")
	w.WriteHeader(http.StatusOK)
	_, _ = io.WriteString(w, "ok\n")
}

func (s *Service) authorized(r *http.Request) bool {
	header := r.Header.Get("Authorization")
	token, ok := strings.CutPrefix(header, "Bearer ")
	if !ok {
		return false
	}
	return subtle.ConstantTimeCompare([]byte(token), []byte(s.token)) == 1
}

// limit applies both the per-machine and the per-source-IP bucket.
func (s *Service) limit(prefix, machine string, r *http.Request, now time.Time) bool {
	s.limiter.prune(now)
	if !s.limiter.allow(prefix+"machine:"+machine, now) {
		return false
	}
	ip := remoteIP(r)
	if ip == "" {
		return true
	}
	return s.limiter.allow(prefix+"ip:"+ip, now)
}

func remoteIP(r *http.Request) string {
	host, _, err := net.SplitHostPort(r.RemoteAddr)
	if err == nil {
		return host
	}
	return r.RemoteAddr
}

func verifyMAC(expected, provided []byte) bool {
	return hmac.Equal(expected, provided)
}

func decodeJSON(w http.ResponseWriter, r *http.Request, dst any) error {
	defer func() { _, _ = io.Copy(io.Discard, r.Body) }()
	dec := json.NewDecoder(http.MaxBytesReader(w, r.Body, maxBodyBytes))
	if err := dec.Decode(dst); err != nil {
		return err
	}
	// Reject a second JSON value in the same body.
	if dec.Decode(&struct{}{}) != io.EOF {
		return errors.New("trailing data after JSON body")
	}
	return nil
}

func writeJSON(w http.ResponseWriter, status int, body any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(body)
}

func writeError(w http.ResponseWriter, status int) {
	writeJSON(w, status, map[string]string{"error": http.StatusText(status)})
}

// logOutcome emits exactly machine, outcome and grantId. It never logs
// nonces, MACs, tokens, IPs or keys.
func (s *Service) logOutcome(event, machine, outcome, grantID string) {
	attrs := []any{"event", event, "outcome", outcome}
	if machine != "" && MachinePattern.MatchString(machine) {
		attrs = append(attrs, "machine", machine)
	}
	if grantID != "" {
		attrs = append(attrs, "grantId", grantID)
	}
	s.logger.Info("wakegrant", attrs...)
}
