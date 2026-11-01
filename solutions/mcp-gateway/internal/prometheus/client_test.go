package prometheus

import (
	"context"
	"net/http"
	"net/http/httptest"
	"testing"
)

func TestQuery(t *testing.T) {
	var gotQuery string
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		gotQuery = r.URL.Query().Get("query")
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write([]byte(`{"status":"success","data":{"resultType":"vector","result":[` +
			`{"metric":{"job":"scheduler"},"value":[1730000000,"0"]},` +
			`{"metric":{"job":"hermes"},"value":[1730000000,"1"]}]}}`))
	}))
	defer srv.Close()

	client := New(srv.URL, srv.Client())
	samples, err := client.Query(context.Background(), `up{job=~"scheduler|hermes"}`, 10)
	if err != nil {
		t.Fatalf("Query: %v", err)
	}
	if gotQuery != `up{job=~"scheduler|hermes"}` {
		t.Fatalf("query = %q", gotQuery)
	}
	if len(samples) != 2 {
		t.Fatalf("samples = %d", len(samples))
	}
	if samples[0].Value != 0 || samples[1].Value != 1 || samples[0].Labels["job"] != "scheduler" {
		t.Fatalf("samples = %+v", samples)
	}
}

func TestQueryLimit(t *testing.T) {
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`{"status":"success","data":{"resultType":"vector","result":[` +
			`{"metric":{},"value":[1,"1"]},{"metric":{},"value":[1,"2"]},{"metric":{},"value":[1,"3"]}]}}`))
	}))
	defer srv.Close()
	samples, err := New(srv.URL, srv.Client()).Query(context.Background(), "up", 2)
	if err != nil {
		t.Fatalf("Query: %v", err)
	}
	if len(samples) != 2 {
		t.Fatalf("samples = %d, want 2", len(samples))
	}
}

func TestQueryErrors(t *testing.T) {
	if _, err := New("http://example", nil).Query(context.Background(), "  ", 1); err == nil {
		t.Fatal("empty query accepted")
	}
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusInternalServerError)
	}))
	defer srv.Close()
	if _, err := New(srv.URL, srv.Client()).Query(context.Background(), "up", 1); err == nil {
		t.Fatal("HTTP 500 accepted")
	}

	srv2 := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte(`{"status":"error","errorType":"bad_data","error":"parse error"}`))
	}))
	defer srv2.Close()
	if _, err := New(srv2.URL, srv2.Client()).Query(context.Background(), "up", 1); err == nil {
		t.Fatal("API error accepted")
	}
}
