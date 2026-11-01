// Package server exposes the gateway's HTTP surface: the authenticated MCP
// endpoint plus unauthenticated cluster-local health and metrics endpoints.
package server

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
	"strings"
	"time"

	"ricardo.local/monorepo/solutions/mcp-gateway/internal/metrics"
)

// Options wires the HTTP server.
type Options struct {
	// AuthToken is the bearer token required on /mcp.
	AuthToken string
	// MCP is the Streamable HTTP handler from the MCP SDK.
	MCP http.Handler
	// Ready reports readiness; nil means always ready.
	Ready func(context.Context) error
	// Metrics and Logger are optional.
	Metrics *metrics.Metrics
	Logger  *slog.Logger
	// Now is the clock used for logs; tests override it.
	Now func() time.Time
}

// Server implements the HTTP routes.
type Server struct{ opts Options }

// New builds a server with safe defaults.
func New(opts Options) *Server {
	if opts.Logger == nil {
		opts.Logger = slog.New(slog.NewTextHandler(io.Discard, nil))
	}
	if opts.Now == nil {
		opts.Now = time.Now
	}
	return &Server{opts: opts}
}

// Handler returns the complete mux.
func (s *Server) Handler() http.Handler {
	mux := http.NewServeMux()
	mux.HandleFunc("GET /healthz", s.handleHealth)
	mux.HandleFunc("GET /readyz", s.handleReady)
	if s.opts.Metrics != nil {
		mux.Handle("GET /metrics", s.opts.Metrics.Handler())
	}
	if s.opts.MCP != nil {
		mux.Handle("/mcp", s.auth(s.opts.MCP))
	}
	return s.observe(mux)
}

func (s *Server) handleHealth(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
}

func (s *Server) handleReady(w http.ResponseWriter, r *http.Request) {
	if s.opts.Ready != nil {
		ctx, cancel := context.WithTimeout(r.Context(), 3*time.Second)
		defer cancel()
		if err := s.opts.Ready(ctx); err != nil {
			s.opts.Logger.Warn("readiness check failed", "error", err)
			writeJSON(w, http.StatusServiceUnavailable, map[string]string{"status": "unavailable"})
			return
		}
	}
	writeJSON(w, http.StatusOK, map[string]string{"status": "ready"})
}

// auth validates "Authorization: Bearer <token>" with a constant-time compare.
func (s *Server) auth(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		header := strings.TrimSpace(r.Header.Get("Authorization"))
		token, hasBearer := strings.CutPrefix(header, "Bearer ")
		token = strings.TrimSpace(token)
		if !hasBearer || token == "" || subtle.ConstantTimeCompare([]byte(token), []byte(s.opts.AuthToken)) != 1 {
			s.opts.Logger.Warn("missing or invalid bearer token", "path", r.URL.Path)
			w.Header().Set("WWW-Authenticate", `Bearer realm="mcp-gateway"`)
			writeJSON(w, http.StatusUnauthorized, map[string]string{"error": "unauthorized"})
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

// Flush lets the MCP streamable handler stream SSE responses through the
// logging wrapper.
func (w *statusWriter) Flush() {
	if flusher, ok := w.ResponseWriter.(http.Flusher); ok {
		flusher.Flush()
	}
}

// Unwrap exposes the original writer to http.ResponseController.
func (w *statusWriter) Unwrap() http.ResponseWriter { return w.ResponseWriter }

func (s *Server) observe(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		start := s.opts.Now()
		sw := &statusWriter{ResponseWriter: w}
		next.ServeHTTP(sw, r)
		status := sw.status
		if status == 0 {
			status = http.StatusOK
		}
		route := routeOf(r.URL.Path)
		if s.opts.Metrics != nil {
			s.opts.Metrics.ObserveHTTP(route, status)
		}
		level := slog.LevelInfo
		if status >= 400 {
			level = slog.LevelWarn
		}
		s.opts.Logger.Log(r.Context(), level, "http request",
			"event", "http_request",
			"http_method", r.Method,
			"http_path", route,
			"http_status", status,
			"duration_ms", s.opts.Now().Sub(start).Milliseconds(),
		)
	})
}

func routeOf(path string) string {
	switch {
	case path == "/mcp":
		return "/mcp"
	case path == "/healthz", path == "/readyz", path == "/metrics":
		return path
	default:
		return "other"
	}
}

func writeJSON(w http.ResponseWriter, status int, value any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	encoder := json.NewEncoder(w)
	encoder.SetEscapeHTML(false)
	_ = encoder.Encode(value)
}
