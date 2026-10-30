package main

import (
	"context"
	"net"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestHealthcheckAddress(t *testing.T) {
	cases := map[string]string{
		"127.0.0.1:8098": "127.0.0.1:8098",
		"0.0.0.0:8098":   "127.0.0.1:8098",
		":8098":          "127.0.0.1:8098",
	}
	for in, want := range cases {
		got, err := healthcheckAddress(in)
		if err != nil {
			t.Fatalf("healthcheckAddress(%q): %v", in, err)
		}
		if got != want {
			t.Fatalf("healthcheckAddress(%q) = %q, want %q", in, got, want)
		}
	}
	if _, err := healthcheckAddress("not-an-address"); err == nil {
		t.Fatal("expected an error for an address without a port")
	}
}

func TestCheckReady(t *testing.T) {
	const token = "probe-token"

	t.Run("ready", func(t *testing.T) {
		srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.URL.Path != "/ricardo.v1.SchedulerService/GetStatus" {
				t.Errorf("path = %q", r.URL.Path)
			}
			if r.Method != http.MethodPost {
				t.Errorf("method = %q", r.Method)
			}
			if got := r.Header.Get("Authorization"); got != "Bearer "+token {
				t.Errorf("authorization header = %q", got)
			}
			w.Header().Set("Content-Type", "application/json")
			w.Write([]byte(`{"ready":true,"lastTickError":""}`))
		}))
		defer srv.Close()
		if err := checkReady(context.Background(), srv.URL, token, srv.Client()); err != nil {
			t.Fatalf("checkReady: %v", err)
		}
	})

	t.Run("not ready", func(t *testing.T) {
		srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.Write([]byte(`{"ready":false,"lastTickError":"tick failed"}`))
		}))
		defer srv.Close()
		err := checkReady(context.Background(), srv.URL, token, srv.Client())
		if err == nil || !strings.Contains(err.Error(), "tick failed") {
			t.Fatalf("expected not-ready error, got %v", err)
		}
	})

	t.Run("unauthorized", func(t *testing.T) {
		srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusUnauthorized)
		}))
		defer srv.Close()
		err := checkReady(context.Background(), srv.URL, token, srv.Client())
		if err == nil || !strings.Contains(err.Error(), "HTTP 401") {
			t.Fatalf("expected HTTP 401 error, got %v", err)
		}
	})

	t.Run("error does not echo the token", func(t *testing.T) {
		ln, err := net.Listen("tcp", "127.0.0.1:0")
		if err != nil {
			t.Fatal(err)
		}
		addr := ln.Addr().String()
		ln.Close()
		err = checkReady(context.Background(), "http://"+addr, token, &http.Client{})
		if err == nil {
			t.Fatal("expected a connection error")
		}
		if strings.Contains(err.Error(), token) {
			t.Fatal("error leaked the token")
		}
	})
}

// TestGetStatusBody proves `scheduler -status` is a faithful, read-only view of
// the authenticated API: the deploy scripts use it instead of a shell inside
// the distroless container.
func TestGetStatusBody(t *testing.T) {
	const token = "status-token"
	const body = `{"ready":true,"queues":[{"name":"control","pending":0,"leased":0}]}`

	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/ricardo.v1.SchedulerService/GetStatus" {
			t.Errorf("path = %q", r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer "+token {
			t.Errorf("authorization header = %q", got)
		}
		w.Header().Set("Content-Type", "application/json")
		w.Write([]byte(body))
	}))
	defer srv.Close()

	got, err := getStatusBody(context.Background(), srv.URL, token, srv.Client())
	if err != nil {
		t.Fatalf("getStatusBody: %v", err)
	}
	if string(got) != body {
		t.Fatalf("body = %q, want %q", got, body)
	}
}

func TestGetStatusBodyRejectsNon200(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.WriteHeader(http.StatusUnauthorized)
	}))
	defer srv.Close()
	if _, err := getStatusBody(context.Background(), srv.URL, "t", srv.Client()); err == nil {
		t.Fatal("expected an error for HTTP 401")
	}
}
