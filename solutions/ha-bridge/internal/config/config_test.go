package config

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"
)

func envWith(overrides map[string]string) func(string) string {
	values := map[string]string{
		EnvHAURL:          "http://ha.example:8123",
		EnvSchedulerURL:   "http://scheduler.example:8098",
		EnvSchedulerToken: "scheduler-secret",
		EnvConsumerID:     "ha-bridge-home",
	}
	for key, value := range overrides {
		if value == "" {
			delete(values, key)
			continue
		}
		values[key] = value
	}
	return func(key string) string { return values[key] }
}

func TestLoadDefaults(t *testing.T) {
	cfg, err := Load(envWith(nil))
	if err != nil {
		t.Fatalf("Load: %v", err)
	}

	if cfg.Execute {
		t.Error("Execute = true, want false (dry-run is the default)")
	}
	if cfg.Queue != QueueHome {
		t.Errorf("Queue = %q, want %q", cfg.Queue, QueueHome)
	}
	if cfg.BatchSize != 10 {
		t.Errorf("BatchSize = %d, want 10", cfg.BatchSize)
	}
	if cfg.LeaseSeconds != 300 {
		t.Errorf("LeaseSeconds = %d, want 300", cfg.LeaseSeconds)
	}
	if cfg.PollInterval != time.Second {
		t.Errorf("PollInterval = %s, want 1s", cfg.PollInterval)
	}
	if cfg.AlarmScript != "alarm_start" || cfg.AnnounceScript != "schedule_announce" || cfg.StopScript != "alarm_stop" {
		t.Errorf("script defaults = %q/%q/%q", cfg.AlarmScript, cfg.AnnounceScript, cfg.StopScript)
	}
	if cfg.HAToken != "" {
		t.Error("HAToken should be empty in dry-run when unset")
	}
}

func TestLoadExecuteRequiresHAToken(t *testing.T) {
	_, err := Load(envWith(map[string]string{EnvExecute: "true"}))
	if err == nil || !strings.Contains(err.Error(), EnvHAToken) {
		t.Fatalf("err = %v, want a missing %s error", err, EnvHAToken)
	}
}

func TestLoadTokenFromFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "ha-token")
	if err := os.WriteFile(path, []byte("file-secret\n"), 0o600); err != nil {
		t.Fatal(err)
	}

	cfg, err := Load(envWith(map[string]string{
		EnvExecute:     "true",
		EnvHAToken:     "env-secret",
		EnvHATokenFile: path,
	}))
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if cfg.HAToken != "file-secret" {
		t.Errorf("HAToken = %q, want the trimmed file value", cfg.HAToken)
	}
}

func TestLoadEmptyTokenFileFails(t *testing.T) {
	path := filepath.Join(t.TempDir(), "empty")
	if err := os.WriteFile(path, []byte("  \n"), 0o600); err != nil {
		t.Fatal(err)
	}
	_, err := Load(envWith(map[string]string{EnvHATokenFile: path}))
	if err == nil || !strings.Contains(err.Error(), "empty") {
		t.Fatalf("err = %v, want an empty token file error", err)
	}
}

func TestLoadRequiresSchedulerToken(t *testing.T) {
	_, err := Load(envWith(map[string]string{EnvSchedulerToken: ""}))
	if err == nil || !strings.Contains(err.Error(), EnvSchedulerToken) {
		t.Fatalf("err = %v, want a missing scheduler token error", err)
	}
}

func TestLoadRejectsURLCredentials(t *testing.T) {
	_, err := Load(envWith(map[string]string{EnvHAURL: "http://user:pass@ha.example:8123"}))
	if err == nil || !strings.Contains(err.Error(), "credentials") {
		t.Fatalf("err = %v, want a credentials error", err)
	}

	_, err = Load(envWith(map[string]string{EnvSchedulerURL: "https://token@example.test"}))
	if err == nil || !strings.Contains(err.Error(), "credentials") {
		t.Fatalf("err = %v, want a credentials error", err)
	}
}

func TestLoadRejectsURLQuery(t *testing.T) {
	_, err := Load(envWith(map[string]string{EnvHAURL: "http://ha.example:8123?token=secret"}))
	if err == nil {
		t.Fatal("query strings must be rejected so secrets cannot hide in URLs")
	}
}

func TestLoadRequiresHTTP(t *testing.T) {
	_, err := Load(envWith(map[string]string{EnvHAURL: "ftp://ha.example"}))
	if err == nil || !strings.Contains(err.Error(), "http") {
		t.Fatalf("err = %v, want a scheme error", err)
	}
}

func TestLoadQueueDefaultsToHome(t *testing.T) {
	cfg, err := Load(envWith(map[string]string{EnvQueue: "  "}))
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if cfg.Queue != "home" {
		t.Errorf("Queue = %q, want home", cfg.Queue)
	}
}

func TestLoadQueueCustom(t *testing.T) {
	cfg, err := Load(envWith(map[string]string{EnvQueue: "home-lab"}))
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if cfg.Queue != "home-lab" {
		t.Errorf("Queue = %q, want home-lab", cfg.Queue)
	}
}

func TestLoadRejectsGenericTaskQueue(t *testing.T) {
	_, err := Load(envWith(map[string]string{EnvQueue: QueueTasks}))
	if err == nil || !strings.Contains(err.Error(), QueueTasks) {
		t.Fatalf("err = %v, want the generic task queue to be rejected", err)
	}
}

func TestLoadRejectsInvalidQueue(t *testing.T) {
	_, err := Load(envWith(map[string]string{EnvQueue: "home queue"}))
	if err == nil {
		t.Fatal("a queue name with a space must be rejected")
	}
}

func TestLoadRejectsShortLease(t *testing.T) {
	_, err := Load(envWith(map[string]string{
		EnvBatchSize:    "200",
		EnvLeaseSeconds: "30",
	}))
	if err == nil || !strings.Contains(err.Error(), "too short") {
		t.Fatalf("err = %v, want a lease-too-short error", err)
	}
}

func TestLoadAcceptsFullServiceURL(t *testing.T) {
	cfg, err := Load(envWith(map[string]string{
		EnvSchedulerURL: "https://scheduler.example/ricardo.v1.SchedulerService",
	}))
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if cfg.SchedulerURL != "https://scheduler.example" {
		t.Errorf("SchedulerURL = %q, want the base URL without the service suffix", cfg.SchedulerURL)
	}
}

func TestLoadRejectsBadConsumerID(t *testing.T) {
	for _, id := range []string{"", "has space", "x/y", strings.Repeat("a", 65)} {
		if _, err := Load(envWith(map[string]string{EnvConsumerID: id})); err == nil {
			t.Errorf("consumer id %q must be rejected", id)
		}
	}
}

func TestLoadRejectsBadValues(t *testing.T) {
	cases := []struct {
		name  string
		value string
	}{
		{EnvExecute, "sometimes"},
		{EnvBatchSize, "0"},
		{EnvBatchSize, "201"},
		{EnvPollInterval, "soon"},
		{EnvMaxBackoff, "1s"}, // smaller than the default poll interval? no: 1s == default
		{EnvHATimeout, "0s"},
		{EnvAlarmScript, "bad script"},
	}
	for _, tc := range cases {
		if tc.name == EnvMaxBackoff {
			// The default poll interval is 1s, so an equal backoff is valid;
			// use a clearly smaller value instead.
			tc.value = "10ms"
		}
		if _, err := Load(envWith(map[string]string{tc.name: tc.value})); err == nil {
			t.Errorf("%s=%q must be rejected", tc.name, tc.value)
		}
	}
}

func TestRequiredLeaseSeconds(t *testing.T) {
	cfg := Config{BatchSize: 10, HATimeout: 10 * time.Second, SchedulerTimeout: 10 * time.Second}
	// 10 * (10s + 10s + 1s) + 15s = 225s
	if got, want := cfg.RequiredLeaseSeconds(), 225; got != want {
		t.Errorf("RequiredLeaseSeconds = %d, want %d", got, want)
	}
}
