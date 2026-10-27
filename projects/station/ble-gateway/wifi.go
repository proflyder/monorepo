package main

import (
	"errors"
	"fmt"
	"os/exec"
	"strconv"
	"strings"
	"time"
	"unicode/utf8"
)

func validateWiFi(ssid, psk string) error {
	if ssid == "" {
		return errors.New("EMPTY_SSID")
	}
	if len([]byte(ssid)) > 32 || !utf8.ValidString(ssid) {
		return errors.New("BAD_SSID")
	}
	for _, r := range ssid {
		if r == 0 || r == '\n' || r == '\r' {
			return errors.New("BAD_SSID")
		}
	}
	if psk == "" {
		return errors.New("EMPTY_PSK")
	}
	if len(psk) == 64 {
		for _, r := range psk {
			if !((r >= '0' && r <= '9') || (r >= 'a' && r <= 'f') || (r >= 'A' && r <= 'F')) {
				return errors.New("BAD_PSK")
			}
		}
		return nil
	}
	if len(psk) < 8 || len(psk) > 63 {
		return errors.New("BAD_PSK_LENGTH")
	}
	for _, r := range psk {
		if r == 0 || r == '\n' || r == '\r' {
			return errors.New("BAD_PSK")
		}
	}
	return nil
}

// wpaCLIPath is a variable so unit tests can substitute a fake CLI.
var wpaCLIPath = "/usr/sbin/wpa_cli"

func wpaCLI(args ...string) (string, error) {
	full := append([]string{"-i", "wlan0"}, args...)
	out, err := exec.Command(wpaCLIPath, full...).CombinedOutput()
	return strings.TrimSpace(string(out)), err
}

// wpa_cli in one-shot argv mode does not accept set_network with SSID/PSK
// values reliably (it returns FAIL). Interactive stdin mode does, and it also
// keeps the PSK out of the process argv.
func wpaCLIInteractive(cmds ...string) (string, error) {
	cmd := exec.Command(wpaCLIPath, "-i", "wlan0")
	cmd.Stdin = strings.NewReader(strings.Join(cmds, "\n") + "\n")
	out, err := cmd.CombinedOutput()
	return strings.TrimSpace(string(out)), err
}

func wpaSetNetwork(id int, field, value string) error {
	out, err := wpaCLIInteractive("set_network " + strconv.Itoa(id) + " " + field + " " + value)
	if err != nil {
		return fmt.Errorf("%v: %s", err, out)
	}
	if !strings.Contains(out, "OK") {
		return fmt.Errorf("not OK: %s", out)
	}
	return nil
}

func wpaState() string {
	out, err := wpaCLI("status")
	if err != nil && out == "" {
		return "UNKNOWN"
	}
	for _, line := range strings.Split(out, "\n") {
		if strings.HasPrefix(line, "wpa_state=") {
			return strings.TrimPrefix(line, "wpa_state=")
		}
	}
	return "UNKNOWN"
}

func wpaCurrentNetwork() int {
	out, err := wpaCLI("list_networks")
	if err != nil {
		return -1
	}
	for _, line := range strings.Split(out, "\n") {
		if !strings.Contains(line, "[CURRENT]") {
			continue
		}
		fields := strings.Split(line, "\t")
		if len(fields) == 0 {
			continue
		}
		id, err := strconv.Atoi(strings.TrimSpace(fields[0]))
		if err == nil {
			return id
		}
	}
	return -1
}

func wpaEnabledNetworks() []int {
	out, err := wpaCLI("list_networks")
	if err != nil {
		return nil
	}
	var ids []int
	for _, line := range strings.Split(out, "\n") {
		if strings.Contains(line, "[DISABLED]") || !strings.Contains(line, "\t") {
			continue
		}
		fields := strings.Split(line, "\t")
		id, err := strconv.Atoi(strings.TrimSpace(fields[0]))
		if err == nil {
			ids = append(ids, id)
		}
	}
	return ids
}

func (g *Gateway) reEnable(ids []int) {
	for _, id := range ids {
		if out, err := wpaCLI("enable_network", strconv.Itoa(id)); err != nil {
			g.logf("re-enable network %d: %v (%s)", id, err, out)
		}
	}
}

func wpaQuote(s string) string {
	var b strings.Builder
	b.WriteByte('"')
	for _, r := range s {
		if r == '\\' || r == '"' {
			b.WriteByte('\\')
		}
		b.WriteRune(r)
	}
	b.WriteByte('"')
	return b.String()
}

func testWPASet() error {
	out, err := wpaCLI("add_network")
	if err != nil {
		return fmt.Errorf("add_network: %v (%s)", err, out)
	}
	id, err := strconv.Atoi(strings.TrimSpace(out))
	if err != nil {
		return fmt.Errorf("add_network output %q", out)
	}
	defer func() { _, _ = wpaCLI("remove_network", strconv.Itoa(id)) }()
	for _, step := range []struct{ field, value string }{
		{"ssid", wpaQuote("Test Net")},
		{"psk", wpaQuote("examplepass")},
		{"key_mgmt", "WPA-PSK"},
	} {
		if err := wpaSetNetwork(id, step.field, step.value); err != nil {
			return fmt.Errorf("set %s: %v", step.field, err)
		}
	}
	out, err = wpaCLI("list_networks")
	if err != nil {
		return err
	}
	return fmt.Errorf("temporary network id=%d set OK; list:\n%s", id, out)
}

func (g *Gateway) applyWiFi(ssid, psk string) {
	g.mu.Lock()
	if g.dryRun {
		g.mu.Unlock()
		g.setStatus("DRY_RUN_OK")
		return
	}
	g.applying = true
	g.mu.Unlock()
	defer func() {
		g.mu.Lock()
		g.applying = false
		g.mu.Unlock()
	}()

	oldID := wpaCurrentNetwork()
	priorEnabled := wpaEnabledNetworks()
	g.logf("Wi-Fi apply: old network id=%d; enabled=%v; trying SSID %q", oldID, priorEnabled, ssid)

	out, err := wpaCLI("add_network")
	if err != nil {
		g.logf("add_network: %v (%s)", err, out)
		g.setStatus("ERROR:ADD_NETWORK")
		return
	}
	id, err := strconv.Atoi(strings.TrimSpace(out))
	if err != nil {
		g.logf("add_network: bad output %q", out)
		g.setStatus("ERROR:ADD_NETWORK")
		return
	}
	g.logf("new network id=%d", id)

	setSteps := []struct {
		field string
		value string
	}{
		{"ssid", wpaQuote(ssid)},
		{"psk", wpaQuote(psk)},
		{"key_mgmt", "WPA-PSK"},
		{"priority", "10"},
	}
	for _, step := range setSteps {
		if err := wpaSetNetwork(id, step.field, step.value); err != nil {
			g.logf("set_network %s: %v", step.field, err)
			g.cleanupNewNetwork(id)
			g.restoreOld(oldID)
			g.setStatus("ERROR:SET_NETWORK")
			return
		}
	}
	if out, err := wpaCLI("enable_network", strconv.Itoa(id)); err != nil {
		g.logf("enable_network: %v (%s)", err, out)
		g.cleanupNewNetwork(id)
		g.restoreOld(oldID)
		g.setStatus("ERROR:ENABLE_NETWORK")
		return
	}
	if out, err := wpaCLI("select_network", strconv.Itoa(id)); err != nil {
		g.logf("select_network: %v (%s)", err, out)
		g.cleanupNewNetwork(id)
		g.restoreOld(oldID)
		g.setStatus("ERROR:SELECT_NETWORK")
		return
	}

	deadline := time.Now().Add(20 * time.Second)
	state := "UNKNOWN"
	for time.Now().Before(deadline) {
		state = wpaState()
		if state == "COMPLETED" {
			break
		}
		time.Sleep(500 * time.Millisecond)
	}
	if state != "COMPLETED" {
		g.logf("new network failed: state=%s; rolling back", state)
		g.cleanupNewNetwork(id)
		g.restoreOld(oldID)
		g.reEnable(priorEnabled)
		g.setStatus("ERROR:TIMEOUT")
		return
	}

	// select_network disables all other profiles. Re-enable the previous one
	// (lower priority) so a reboot still has a fallback if the new network is
	// unavailable, then persist both.
	g.reEnable(priorEnabled)
	if out, err := wpaCLI("save_config"); err != nil {
		g.logf("save_config: %v (%s)", err, out)
		// The network is connected; do not tear it down for a save failure.
	} else {
		g.logf("saved Wi-Fi config")
	}
	g.logf("Wi-Fi connected: SSID %q", ssid)
	g.setStatus("CONNECTED")

	// Give the phone time to read the status. Auto mode will stop advertising
	// on its next tick when manual is not active.
	time.Sleep(3 * time.Second)
}

func (g *Gateway) cleanupNewNetwork(id int) {
	if id < 0 {
		return
	}
	out, err := wpaCLI("remove_network", strconv.Itoa(id))
	if err != nil {
		g.logf("remove_network %d: %v (%s)", id, err, out)
	}
}

func (g *Gateway) restoreOld(id int) {
	if id < 0 {
		g.logf("no previous network to restore")
		return
	}
	out, err := wpaCLI("select_network", strconv.Itoa(id))
	if err != nil {
		g.logf("restore old %d: %v (%s)", id, err, out)
		return
	}
	g.logf("restored old network id=%d", id)
	_ = out
}
