package loki

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func TestQuery(t *testing.T) {
	var gotSelector, gotDirection string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotSelector = r.URL.Query().Get("query")
		gotDirection = r.URL.Query().Get("direction")
		_, _ = w.Write([]byte(`{"status":"success","data":{"resultType":"streams","result":[{"stream":{"namespace":"home"},` +
			`"values":[["1730000000000000000","first"],["1730000001000000000","second"]]}]}}`))
	}))
	defer srv.Close()

	client := New(srv.URL, srv.Client())
	lines, err := client.Query(context.Background(), `{namespace="home"}`, 15*time.Minute, 50)
	if err != nil {
		t.Fatalf("Query: %v", err)
	}
	if gotSelector != `{namespace="home"}` || gotDirection != "backward" {
		t.Fatalf("selector = %q, direction = %q", gotSelector, gotDirection)
	}
	if len(lines) != 2 {
		t.Fatalf("lines = %d", len(lines))
	}
	if lines[0].Text != "second" || lines[1].Text != "first" {
		t.Fatalf("lines not newest-first: %+v", lines)
	}
	if lines[0].Labels["namespace"] != "home" {
		t.Fatalf("labels = %+v", lines[0].Labels)
	}
}

func TestQueryErrors(t *testing.T) {
	if _, err := New("http://example", nil).Query(context.Background(), "", time.Minute, 10); err == nil {
		t.Fatal("empty selector accepted")
	}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`{"status":"error","error":"parse error"}`))
	}))
	defer srv.Close()
	if _, err := New(srv.URL, srv.Client()).Query(context.Background(), `{a="b"}`, time.Minute, 10); err == nil {
		t.Fatal("Loki API error accepted")
	}
}
