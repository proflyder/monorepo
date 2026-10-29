// Package catalogapi serves the read-only control catalog to the phone.
//
// GET /v1/catalog returns the entries discovered from Home Assistant with
// their live state. It has no write surface: commands go through the scheduler
// (the phone enqueues a task), never through this server.
package catalogapi

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"log/slog"
	"net/http"
	"strings"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/discovery"
)

// Source yields the discovered catalog.
type Source interface {
	Get(ctx context.Context) (discovery.Snapshot, error)
}

// EntryView is the wire shape of one catalog entry.
type EntryView struct {
	ID      string   `json:"id"`
	Kind    string   `json:"kind"`
	Title   string   `json:"title"`
	Room    string   `json:"room,omitempty"`
	Icon    string   `json:"icon,omitempty"`
	Actions []string `json:"actions"`
	// State is "on"/"off" for devices; "" for scenarios.
	State string `json:"state,omitempty"`
	// Available is false when Home Assistant reports the entity missing or
	// unavailable, or could not be reached.
	Available bool `json:"available"`
}

// Response is the wire shape of GET /v1/catalog.
type Response struct {
	Entries []EntryView `json:"entries"`
	// StatesOK is false when Home Assistant could not be read; entries are then
	// listed without state and flagged unavailable.
	StatesOK bool `json:"statesOk"`
}

// Handler builds the HTTP handler. token must be non-empty.
func Handler(src Source, token string, logger *slog.Logger) http.Handler {
	if logger == nil {
		logger = slog.New(slog.DiscardHandler)
	}
	mux := http.NewServeMux()
	mux.HandleFunc("GET /v1/catalog", func(w http.ResponseWriter, r *http.Request) {
		if !authorized(r, token) {
			w.Header().Set("WWW-Authenticate", "Bearer")
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		snap, err := src.Get(r.Context())
		if err != nil {
			logger.Warn("catalog discovery failed", "error", err)
			http.Error(w, "home assistant is not reachable", http.StatusBadGateway)
			return
		}
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("Cache-Control", "no-store")
		_ = json.NewEncoder(w).Encode(build(snap))
	})
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	})
	return mux
}

func build(snap discovery.Snapshot) Response {
	entries := snap.Catalog.Entries()
	resp := Response{Entries: make([]EntryView, 0, len(entries)), StatesOK: snap.StatesOK}
	for _, e := range entries {
		resp.Entries = append(resp.Entries, EntryView{
			ID: e.ID, Kind: string(e.Kind), Title: e.Title, Room: e.Room, Icon: e.Icon,
			Actions: e.Actions(), State: e.State, Available: e.Available,
		})
	}
	return resp
}

func authorized(r *http.Request, token string) bool {
	header := r.Header.Get("Authorization")
	got, ok := strings.CutPrefix(header, "Bearer ")
	if !ok || token == "" {
		return false
	}
	return subtle.ConstantTimeCompare([]byte(got), []byte(token)) == 1
}
