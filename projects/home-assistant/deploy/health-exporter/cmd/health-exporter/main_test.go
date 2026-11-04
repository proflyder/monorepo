package main

import (
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"testing"
	"time"
)

// TestHACanBeCheckedOnSeparateVM is the port of test_health_exporter.py: the HA
// host may live on a different machine, everything else defaults to loopback.
func TestHACanBeCheckedOnSeparateVM(t *testing.T) {
	t.Setenv("ENDPOINT_HOST_HA", "haos.home.arpa")
	var dialled []string
	exporter := New()
	exporter.Dial = func(host string, port int) error {
		dialled = append(dialled, host+":"+strconv.Itoa(port))
		return nil
	}
	body := string(exporter.Metrics())
	if !strings.Contains(body, `ricardo_endpoint_up{endpoint="ha"} 1`) {
		t.Fatalf("ha is not up: %s", body)
	}
	if !containsString(dialled, "haos.home.arpa:8123") {
		t.Fatalf("ha was not checked on its own host: %v", dialled)
	}
	if !containsString(dialled, "127.0.0.1:8095") {
		t.Fatalf("music_assistant did not default to loopback: %v", dialled)
	}
}

func TestEveryEndpointHasBothMetrics(t *testing.T) {
	exporter := New()
	exporter.Dial = func(string, int) error { return nil }
	body := string(exporter.Metrics())
	if !strings.Contains(body, "# TYPE ricardo_endpoint_up gauge") ||
		!strings.Contains(body, "# TYPE ricardo_endpoint_connect_seconds gauge") {
		t.Fatal("metric help/type lines are missing")
	}
	for _, endpoint := range Endpoints {
		if !strings.Contains(body, `ricardo_endpoint_up{endpoint="`+endpoint.Name+`"} 1`) {
			t.Fatalf("%s up metric is missing", endpoint.Name)
		}
		if !strings.Contains(body, `ricardo_endpoint_connect_seconds{endpoint="`+endpoint.Name+`"} `) {
			t.Fatalf("%s duration metric is missing", endpoint.Name)
		}
	}
}

func TestFailedDialIsDown(t *testing.T) {
	exporter := New()
	exporter.Dial = func(host string, port int) error {
		if port == 8643 {
			return errDial
		}
		return nil
	}
	body := string(exporter.Metrics())
	if !strings.Contains(body, `ricardo_endpoint_up{endpoint="voice_proxy"} 0`) {
		t.Fatalf("voice_proxy should be down: %s", body)
	}
}

func TestConnectSecondsAreFormattedWithSixDecimals(t *testing.T) {
	exporter := New()
	exporter.Dial = func(string, int) error { return nil }
	times := []time.Time{time.Unix(0, 0), time.Unix(0, 1234000)}
	index := 0
	exporter.Now = func() time.Time {
		value := times[index%len(times)]
		index++
		return value
	}
	body := string(exporter.Metrics())
	if !strings.Contains(body, `ricardo_endpoint_connect_seconds{endpoint="ha"} 0.001234`) {
		t.Fatalf("duration is not %.6f: %s", 0.001234, body)
	}
}

func TestHandlerServesMetricsAndRejectsOtherPaths(t *testing.T) {
	exporter := New()
	exporter.Dial = func(string, int) error { return nil }
	server := httptest.NewServer(exporter.Handler())
	defer server.Close()

	response, err := http.Get(server.URL + "/metrics")
	if err != nil {
		t.Fatal(err)
	}
	defer response.Body.Close()
	if response.StatusCode != http.StatusOK {
		t.Fatalf("status = %d", response.StatusCode)
	}
	if got := response.Header.Get("Content-Type"); got != "text/plain; version=0.0.4" {
		t.Fatalf("content type = %q", got)
	}
	other, err := http.Get(server.URL + "/healthz")
	if err != nil {
		t.Fatal(err)
	}
	defer other.Body.Close()
	if other.StatusCode != http.StatusNotFound {
		t.Fatalf("non-/metrics status = %d", other.StatusCode)
	}
}

var errDial = &dialError{}

type dialError struct{}

func (*dialError) Error() string { return "connection refused" }

func containsString(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}
