// Package server exposes the internal JSON API (health, metrics, history,
// trigger) and the dedicated Telegram webhook listener.
package server

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"strconv"
	"strings"
	"time"

	"ricardo.local/monorepo/solutions/currency-bot/internal/metrics"
	"ricardo.local/monorepo/solutions/currency-bot/internal/model"
	"ricardo.local/monorepo/solutions/currency-bot/internal/service"
	"ricardo.local/monorepo/solutions/currency-bot/internal/store"
)

// HistoryStore is the subset of the store used by the HTTP API.
type HistoryStore interface {
	AllRecords(ctx context.Context) ([]model.Record, error)
	RecordsByDateRange(ctx context.Context, from, to time.Time) ([]model.Record, error)
	LatestRecord(ctx context.Context) (model.Record, error)
	DeleteAll(ctx context.Context) (int, error)
	Ping(ctx context.Context) error
}

// Sender delivers Telegram replies to command authors.
type Sender interface {
	Send(ctx context.Context, chatID, text string) error
}

// Options wires the HTTP layer.
type Options struct {
	APIToken      string
	WebhookSecret string
	AllowedChats  []string
	DefaultChatID string
	Pipeline      *service.Service
	History       HistoryStore
	Sender        Sender
	Metrics       *metrics.Metrics
	Logger        *slog.Logger
	Now           func() time.Time
}

// Server implements both listeners.
type Server struct {
	opts    Options
	allowed map[string]struct{}
}

// New builds a server; defaults are filled in for optional dependencies.
func New(opts Options) *Server {
	if opts.Logger == nil {
		opts.Logger = slog.New(slog.NewTextHandler(io.Discard, nil))
	}
	if opts.Now == nil {
		opts.Now = time.Now
	}
	allowed := make(map[string]struct{}, len(opts.AllowedChats))
	for _, chat := range opts.AllowedChats {
		allowed[strings.TrimSpace(chat)] = struct{}{}
	}
	return &Server{opts: opts, allowed: allowed}
}

// APIHandler returns the main listener handler.
func (s *Server) APIHandler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /", s.handleRoot)
	mux.HandleFunc("GET /healthz", s.handleHealth)
	mux.HandleFunc("GET /readyz", s.handleReady)
	mux.Handle("GET /metrics", s.opts.Metrics.Handler())
	mux.Handle("GET /api/history", s.auth(http.HandlerFunc(s.handleHistory)))
	mux.Handle("GET /api/latest", s.auth(http.HandlerFunc(s.handleLatest)))
	mux.Handle("DELETE /api/history", s.auth(http.HandlerFunc(s.handleDeleteHistory)))
	mux.Handle("POST /api/trigger", s.auth(http.HandlerFunc(s.handleTrigger)))
	return s.observe(mux)
}

// WebhookHandler returns the dedicated webhook listener handler: exactly one
// route, nothing else.
func (s *Server) WebhookHandler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("POST /telegram/webhook", s.handleWebhook)
	return s.observe(mux)
}

func (s *Server) handleRoot(w http.ResponseWriter, _ *http.Request) {
	w.Header().Set("Content-Type", "text/plain; charset=UTF-8")
	_, _ = io.WriteString(w, "Hello World!")
}

func (s *Server) handleHealth(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
}

func (s *Server) handleReady(w http.ResponseWriter, r *http.Request) {
	if err := s.opts.History.Ping(r.Context()); err != nil {
		s.opts.Logger.Error("readiness check failed", "error", err)
		writeJSON(w, http.StatusServiceUnavailable, map[string]string{"status": "unavailable"})
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"status": "ready"})
}

// auth validates Authorization: Bearer <API_TOKEN> with a constant-time compare.
func (s *Server) auth(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		header := strings.TrimSpace(r.Header.Get("Authorization"))
		token, hasBearer := strings.CutPrefix(header, "Bearer ")
		token = strings.TrimSpace(token)
		if !hasBearer || token == "" || subtle.ConstantTimeCompare([]byte(token), []byte(s.opts.APIToken)) != 1 {
			s.opts.Logger.Warn("missing or invalid api token", "path", r.URL.Path)
			s.writeError(w, r, http.StatusUnauthorized, "Unauthorized",
				"Authentication required: Invalid or missing API key", "UNAUTHORIZED", "auth", nil)
			return
		}
		next.ServeHTTP(w, r)
	})
}

type statusWriter struct {
	http.ResponseWriter
	status int
}

func (w *statusWriter) WriteHeader(status int) {
	w.status = status
	w.ResponseWriter.WriteHeader(status)
}

func (w *statusWriter) Write(p []byte) (int, error) {
	if w.status == 0 {
		w.status = http.StatusOK
	}
	return w.ResponseWriter.Write(p)
}

// observe logs every request as JSON and counts it by path and status. It never
// logs request bodies, so webhook content stays private.
func (s *Server) observe(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := time.Now()
		sw := &statusWriter{ResponseWriter: w}
		next.ServeHTTP(sw, r)
		status := sw.status
		if status == 0 {
			status = http.StatusOK
		}
		if s.opts.Metrics != nil {
			s.opts.Metrics.ObserveHTTP(r.URL.Path, status)
		}
		level := slog.LevelInfo
		if status >= 400 {
			level = slog.LevelWarn
		}
		s.opts.Logger.Log(r.Context(), level, "http request",
			"event", "http_request",
			"http_method", r.Method,
			"http_path", r.URL.Path,
			"http_status", status,
			"duration_ms", time.Since(start).Milliseconds(),
		)
	})
}

// errorResponse is the compatibility envelope for API failures.
type errorResponse struct {
	Timestamp string            `json:"timestamp"`
	Status    int               `json:"status"`
	Error     string            `json:"error"`
	Message   string            `json:"message"`
	Path      string            `json:"path"`
	ErrorCode string            `json:"errorCode,omitempty"`
	Service   string            `json:"service,omitempty"`
	RequestID string            `json:"requestId,omitempty"`
	Details   map[string]string `json:"details,omitempty"`
}

func (s *Server) writeError(w http.ResponseWriter, r *http.Request, status int, errName, message, code, serviceName string, details map[string]string) {
	writeJSON(w, status, errorResponse{
		Timestamp: s.opts.Now().UTC().Format(time.RFC3339Nano),
		Status:    status,
		Error:     errName,
		Message:   message,
		Path:      r.URL.Path,
		ErrorCode: code,
		Service:   serviceName,
		Details:   details,
	})
}

func writeJSON(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	encoder := json.NewEncoder(w)
	encoder.SetEscapeHTML(false)
	_ = encoder.Encode(value)
}

func (s *Server) isAllowedChat(chatID int64) bool {
	_, ok := s.allowed[strconv.FormatInt(chatID, 10)]
	return ok
}

func isNotFound(err error) bool { return errors.Is(err, store.ErrNotFound) }

func itoa(n int) string     { return strconv.Itoa(n) }
func itoa64(n int64) string { return strconv.FormatInt(n, 10) }
