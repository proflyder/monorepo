package main

import (
	"crypto/rand"
	"crypto/subtle"
	"encoding/hex"
	"encoding/json"
	"log"
	"net/http"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// Local HTTP control for Home Assistant voice intents.
//
// The endpoint is LAN-only by policy and requires a bearer token. The token
// lives in /data/quasar/ble-gateway/control-token (0600) and in HA secrets.
func (g *Gateway) loadControlToken(path string) error {
	if b, err := os.ReadFile(path); err == nil {
		if t := strings.TrimSpace(string(b)); len(t) >= 16 {
			g.controlToken = t
			return nil
		}
	}
	buf := make([]byte, 32)
	if _, err := rand.Read(buf); err != nil {
		return err
	}
	g.controlToken = hex.EncodeToString(buf)
	if err := os.MkdirAll(filepath.Dir(path), 0700); err != nil {
		return err
	}
	if err := os.WriteFile(path, []byte(g.controlToken+"\n"), 0600); err != nil {
		return err
	}
	return nil
}

func (g *Gateway) startControl(addr, tokenPath string) error {
	if err := g.loadControlToken(tokenPath); err != nil {
		return err
	}
	mux := http.NewServeMux()
	mux.HandleFunc("/status", g.controlAuth(g.handleStatus))
	mux.HandleFunc("/gateway/on", g.controlAuth(g.handleGatewayOn))
	mux.HandleFunc("/gateway/off", g.controlAuth(g.handleGatewayOff))
	mux.HandleFunc("/bluetooth/on", g.controlAuth(g.handleBluetoothOn))
	mux.HandleFunc("/bluetooth/off", g.controlAuth(g.handleBluetoothOff))
	srv := &http.Server{
		Addr:              addr,
		Handler:           mux,
		ReadHeaderTimeout: 5 * time.Second,
	}
	go func() {
		log.Printf("control HTTP listening on %s", addr)
		if err := srv.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			log.Printf("control HTTP stopped: %v", err)
		}
	}()
	return nil
}

func (g *Gateway) controlAuth(next http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		got := r.Header.Get("Authorization")
		want := "Bearer " + g.controlToken
		if len(got) != len(want) || subtle.ConstantTimeCompare([]byte(got), []byte(want)) != 1 {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		next(w, r)
	}
}

func (g *Gateway) controlStatus() map[string]any {
	g.mu.Lock()
	status := g.status
	manual := g.manualUntil.After(time.Now())
	bt := g.btMode
	g.mu.Unlock()
	return map[string]any{
		"status":    status,
		"manual":    manual,
		"bluetooth": bt,
		"wifi":      wpaState(),
	}
}

func writeControlJSON(w http.ResponseWriter, code int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	_ = json.NewEncoder(w).Encode(v)
}

func (g *Gateway) handleStatus(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodGet {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	writeControlJSON(w, http.StatusOK, g.controlStatus())
}

func (g *Gateway) handleGatewayOn(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	g.openManual()
	writeControlJSON(w, http.StatusOK, g.controlStatus())
}

func (g *Gateway) handleGatewayOff(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	g.closeManual()
	writeControlJSON(w, http.StatusOK, g.controlStatus())
}

func (g *Gateway) handleBluetoothOn(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	g.enableBluetooth()
	writeControlJSON(w, http.StatusOK, g.controlStatus())
}

func (g *Gateway) handleBluetoothOff(w http.ResponseWriter, r *http.Request) {
	if r.Method != http.MethodPost {
		http.Error(w, "method not allowed", http.StatusMethodNotAllowed)
		return
	}
	g.disableBluetooth()
	writeControlJSON(w, http.StatusOK, g.controlStatus())
}
