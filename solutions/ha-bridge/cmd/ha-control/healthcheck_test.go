package main

import (
	"context"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestHealthcheckAddress(t *testing.T) {
	cases := map[string]string{
		"127.0.0.1:8099": "127.0.0.1:8099",
		"0.0.0.0:8099":   "127.0.0.1:8099",
		":8099":          "127.0.0.1:8099",
	}
	for in, want := range cases {
		got, err := healthcheckAddress(in)
		if err != nil || got != want {
			t.Fatalf("healthcheckAddress(%q) = %q, %v; want %q", in, got, err, want)
		}
	}
}

func TestCheckCatalog(t *testing.T) {
	const token = "probe-token"
	t.Run("ok", func(t *testing.T) {
		srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.URL.Path != "/v1/catalog" {
				t.Errorf("path = %q", r.URL.Path)
			}
			if got := r.Header.Get("Authorization"); got != "Bearer "+token {
				t.Errorf("authorization = %q", got)
			}
			w.Write([]byte(`{"entries":[],"statesOk":true}`))
		}))
		defer srv.Close()
		if err := checkCatalog(context.Background(), srv.URL, token, srv.Client()); err != nil {
			t.Fatal(err)
		}
	})
	t.Run("unauthorized", func(t *testing.T) {
		srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			w.WriteHeader(http.StatusUnauthorized)
		}))
		defer srv.Close()
		err := checkCatalog(context.Background(), srv.URL, token, srv.Client())
		if err == nil || !strings.Contains(err.Error(), "HTTP 401") {
			t.Fatalf("expected 401 error, got %v", err)
		}
	})
}
