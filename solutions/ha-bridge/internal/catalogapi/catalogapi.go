// Package catalogapi serves the read-only control catalog to the phone.
//
// GET /v1/catalog returns the allowlisted entries with their live Home
// Assistant state. It has no write surface: commands go through the scheduler
// (the phone enqueues a task), never through this server.
package catalogapi

import (
	"context"
	"crypto/subtle"
	"encoding/json"
	"log/slog"
	"net/http"
	"strings"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/catalog"
)

// States reads Home Assistant entity states.
type States interface {
	States(ctx context.Context) (map[string]string, error)
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
func Handler(cat *catalog.Catalog, states States, token string, logger *slog.Logger) http.Handler {
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
		resp := build(r.Context(), cat, states, logger)
		w.Header().Set("Content-Type", "application/json")
		w.Header().Set("Cache-Control", "no-store")
		_ = json.NewEncoder(w).Encode(resp)
	})
	mux.HandleFunc("GET /healthz", func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusNoContent)
	})
	return mux
}

func build(ctx context.Context, cat *catalog.Catalog, states States, logger *slog.Logger) Response {
	live, err := states.States(ctx)
	statesOK := err == nil
	if err != nil {
		logger.Warn("home assistant states unavailable", "error", err)
	}
	entries := cat.Entries()
	resp := Response{Entries: make([]EntryView, 0, len(entries)), StatesOK: statesOK}
	for _, e := range entries {
		view := EntryView{
			ID: e.ID, Kind: string(e.Kind), Title: e.Title, Room: e.Room, Icon: e.Icon,
			Actions: e.Actions(),
		}
		if statesOK {
			state, present := live[e.Entity]
			view.Available = present && state != "unavailable" && state != "unknown"
			if e.Kind != catalog.KindScenario && view.Available {
				view.State = state
			}
		}
		resp.Entries = append(resp.Entries, view)
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
