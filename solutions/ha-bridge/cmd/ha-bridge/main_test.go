package main

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestRunDryRunEndToEnd(t *testing.T) {
	var schedulerPaths, haPaths []string

	schedulerServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		schedulerPaths = append(schedulerPaths, r.URL.Path)
		if got := r.Header.Get("Authorization"); got != "Bearer scheduler-secret" {
			t.Errorf("scheduler Authorization = %q", got)
		}
		if !strings.HasSuffix(r.URL.Path, "/GetStatus") {
			t.Errorf("unexpected scheduler call %s; dry-run must not lease events", r.URL.Path)
		}
		fmt.Fprint(w, `{"version":"test-1","pendingEvents":1,"queues":[{"name":"home","pending":1}]}`)
	}))
	defer schedulerServer.Close()

	haServer := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		haPaths = append(haPaths, r.Method+" "+r.URL.Path)
		if got := r.Header.Get("Authorization"); got != "Bearer ha-secret" {
			t.Errorf("HA Authorization = %q", got)
		}
		if r.Method != http.MethodGet || r.URL.Path != "/api/" {
			t.Errorf("unexpected HA call %s %s; dry-run must not call services", r.Method, r.URL.Path)
		}
		fmt.Fprint(w, `{"message":"API running."}`)
	}))
	defer haServer.Close()

	env := map[string]string{
		"HA_URL":             haServer.URL,
		"SCHEDULER_ENDPOINT": schedulerServer.URL,
		"SCHEDULER_TOKEN":    "scheduler-secret",
		"HA_TOKEN":           "ha-secret",
		"BRIDGE_CONSUMER_ID": "ha-bridge-home",
	}
	getenv := func(key string) string { return env[key] }

	if err := run(context.Background(), getenv, io.Discard); err != nil {
		t.Fatalf("run: %v", err)
	}

	if len(schedulerPaths) != 1 || schedulerPaths[0] != "/ricardo.v1.SchedulerService/GetStatus" {
		t.Errorf("scheduler calls = %v, want only GetStatus", schedulerPaths)
	}
	if len(haPaths) != 1 || haPaths[0] != "GET /api/" {
		t.Errorf("HA calls = %v, want only the read-only probe", haPaths)
	}
}

func TestRunRejectsInvalidConfiguration(t *testing.T) {
	env := map[string]string{"HA_URL": "http://ha.example:8123"}
	getenv := func(key string) string { return env[key] }

	if err := run(context.Background(), getenv, io.Discard); err == nil {
		t.Fatal("run must fail without a scheduler endpoint, token and consumer id")
	}
}

func TestRunExecuteWithoutHAClientFails(t *testing.T) {
	// Execute mode with a token is required, so the missing-client guard is
	// covered by the bridge package tests; here we only assert that an
	// invalid execute flag is rejected before any network call.
	env := map[string]string{
		"HA_URL":             "http://ha.example:8123",
		"SCHEDULER_ENDPOINT": "http://scheduler.example:8098",
		"SCHEDULER_TOKEN":    "scheduler-secret",
		"BRIDGE_CONSUMER_ID": "ha-bridge-home",
		"BRIDGE_EXECUTE":     "maybe",
	}
	getenv := func(key string) string { return env[key] }
	if err := run(context.Background(), getenv, io.Discard); err == nil {
		t.Fatal("an invalid BRIDGE_EXECUTE value must be rejected")
	}
}
