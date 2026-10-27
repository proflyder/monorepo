package main

import (
	"os"
	"path/filepath"
	"reflect"
	"strings"
	"testing"
)

func TestValidateWiFi(t *testing.T) {
	cases := []struct {
		name, ssid, psk string
		wantErr         string
	}{
		{"ok", "Home", "password123", ""},
		{"ok hex 64", "Home", strings.Repeat("a", 64), ""},
		{"empty ssid", "", "password123", "EMPTY_SSID"},
		{"ssid too long", strings.Repeat("x", 33), "password123", "BAD_SSID"},
		{"ssid newline", "Home\n", "password123", "BAD_SSID"},
		{"empty psk", "Home", "", "EMPTY_PSK"},
		{"short psk", "Home", "short", "BAD_PSK_LENGTH"},
		{"long psk", "Home", strings.Repeat("x", 64), "BAD_PSK"},
		{"psk newline", "Home", "password\n123", "BAD_PSK"},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			err := validateWiFi(tc.ssid, tc.psk)
			if tc.wantErr == "" {
				if err != nil {
					t.Fatalf("unexpected error: %v", err)
				}
				return
			}
			if err == nil || err.Error() != tc.wantErr {
				t.Fatalf("error = %v, want %q", err, tc.wantErr)
			}
		})
	}
}

func TestWpaQuote(t *testing.T) {
	cases := map[string]string{
		"Home":   `"Home"`,
		`a"b`:    `"a\"b"`,
		`a\b`:    `"a\\b"`,
		"Дом 5G": `"Дом 5G"`,
	}
	for in, want := range cases {
		if got := wpaQuote(in); got != want {
			t.Errorf("wpaQuote(%q) = %q, want %q", in, got, want)
		}
	}
}

func installFakeWPA(t *testing.T, body string) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "wpa_cli")
	if err := os.WriteFile(path, []byte("#!/bin/sh\n"+body+"\n"), 0755); err != nil {
		t.Fatal(err)
	}
	old := wpaCLIPath
	wpaCLIPath = path
	t.Cleanup(func() { wpaCLIPath = old })
}

func TestWpaQueriesAndSetNetwork(t *testing.T) {
	installFakeWPA(t, `
case "$*" in
  *" status"*) echo wpa_state=COMPLETED ;;
  *" list_networks"*) printf '0\tHome\tany\t[CURRENT]\n1\tOld\tany\t[DISABLED]\n2\tWork\tany\n' ;;
  *" add_network"*) echo 7 ;;
  *) echo OK ;;
esac`)
	if got := wpaState(); got != "COMPLETED" {
		t.Fatalf("wpaState = %q", got)
	}
	if got := wpaCurrentNetwork(); got != 0 {
		t.Fatalf("wpaCurrentNetwork = %d", got)
	}
	if got := wpaEnabledNetworks(); !reflect.DeepEqual(got, []int{0, 2}) {
		t.Fatalf("wpaEnabledNetworks = %v", got)
	}
	if err := wpaSetNetwork(7, "ssid", wpaQuote("Test")); err != nil {
		t.Fatalf("wpaSetNetwork: %v", err)
	}
	if err := testWPASet(); err == nil || !strings.Contains(err.Error(), "temporary network id=7") {
		t.Fatalf("testWPASet error = %v", err)
	}
}

func TestWpaUnknownWhenCLIEmpty(t *testing.T) {
	installFakeWPA(t, `exit 1`)
	if got := wpaState(); got != "UNKNOWN" {
		t.Fatalf("wpaState = %q, want UNKNOWN", got)
	}
	if got := wpaCurrentNetwork(); got != -1 {
		t.Fatalf("wpaCurrentNetwork = %d, want -1", got)
	}
	if got := wpaEnabledNetworks(); got != nil {
		t.Fatalf("wpaEnabledNetworks = %v, want nil", got)
	}
}

func TestApplyWiFiSuccess(t *testing.T) {
	installFakeWPA(t, `
case "$*" in
  *" status"*) echo wpa_state=COMPLETED ;;
  *" list_networks"*) printf '0\tHome\tany\t[CURRENT]\n1\tFallback\tany\n' ;;
  *" add_network"*) echo 7 ;;
  *" enable_network"*) echo OK ;;
  *" select_network"*) echo OK ;;
  *" save_config"*) echo OK ;;
  *) echo OK ;;
esac`)
	g := newTestGateway(t)
	g.dryRun = false
	g.applyWiFi("Home", "password123")
	if got := strings.TrimSpace(readFile(t, g.statusFile)); got != "CONNECTED" {
		t.Fatalf("status = %q, want CONNECTED", got)
	}
}

func TestApplyWiFiAddNetworkFailure(t *testing.T) {
	installFakeWPA(t, `
case "$*" in
  *" list_networks"*) printf '0\tHome\tany\t[CURRENT]\n' ;;
  *" add_network"*) echo FAIL; exit 1 ;;
esac`)
	g := newTestGateway(t)
	g.dryRun = false
	g.applyWiFi("Home", "password123")
	if got := strings.TrimSpace(readFile(t, g.statusFile)); got != "ERROR:ADD_NETWORK" {
		t.Fatalf("status = %q, want ERROR:ADD_NETWORK", got)
	}
}

func TestApplyWiFiSetNetworkFailureRollsBack(t *testing.T) {
	installFakeWPA(t, `
case "$*" in
  *" list_networks"*) printf '0\tHome\tany\t[CURRENT]\n1\tFallback\tany\n' ;;
  *" add_network"*) echo 7 ;;
  *" set_network "*) echo FAIL; exit 1 ;;
  *" select_network "*) echo OK ;;
esac`)
	g := newTestGateway(t)
	g.dryRun = false
	g.applyWiFi("Home", "password123")
	if got := strings.TrimSpace(readFile(t, g.statusFile)); got != "ERROR:SET_NETWORK" {
		t.Fatalf("status = %q, want ERROR:SET_NETWORK", got)
	}
}
