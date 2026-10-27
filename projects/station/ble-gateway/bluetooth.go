package main

import (
	"log"
	"os/exec"
	"strings"
	"time"
)

// Bluetooth A2DP sink mode for the stock "mute x4" gesture.
//
// The firmware already ships bluealsad + bluealsa-aplay and starts them at
// boot, so the only missing pieces are discoverability and a pairing agent.
// A discoverable classic adapter can disturb Wi-Fi on the combo chip, so the
// pairing window is limited to 90 seconds; an already paired phone keeps its
// A2DP connection, and Wi-Fi is asked to reconnect when the window closes.
const bluetoothWindow = 90 * time.Second

func bluetoothctl(args ...string) (string, error) {
	out, err := exec.Command("/usr/bin/bluetoothctl", args...).CombinedOutput()
	return strings.TrimSpace(string(out)), err
}

func processRunning(name string) bool {
	out, err := exec.Command("/bin/pidof", name).Output()
	return err == nil && strings.TrimSpace(string(out)) != ""
}

func ensureBluetoothAudio() {
	if !processRunning("bluealsad") {
		if out, err := exec.Command("/etc/init.d/S52bluealsa", "start").CombinedOutput(); err != nil {
			log.Printf("bluetooth: bluealsad start: %v (%s)", err, strings.TrimSpace(string(out)))
		}
	}
	if !processRunning("bluealsa-aplay") {
		if out, err := exec.Command("/etc/init.d/S53bluealsa-aplay", "start").CombinedOutput(); err != nil {
			log.Printf("bluetooth: bluealsa-aplay start: %v (%s)", err, strings.TrimSpace(string(out)))
		}
	}
	if !processRunning("bt-agent") {
		// NoInputNoOutput is enough for a phone acting as an A2DP source.
		if out, err := exec.Command("/usr/bin/bt-agent", "-c", "NoInputNoOutput", "-d").CombinedOutput(); err != nil {
			log.Printf("bluetooth: bt-agent start: %v (%s)", err, strings.TrimSpace(string(out)))
		}
	}
}

func (g *Gateway) wifiReconnect() {
	if out, err := wpaCLI("reconnect"); err != nil {
		g.logf("bluetooth: wpa reconnect: %v (%s)", err, out)
		return
	}
	g.logf("bluetooth: asked wpa_supplicant to reconnect Wi-Fi")
}

func (g *Gateway) disconnectBluetoothDevices() {
	out, _ := bluetoothctl("devices", "Connected")
	for _, line := range strings.Split(out, "\n") {
		fields := strings.Fields(line)
		if len(fields) >= 3 && strings.EqualFold(fields[0], "Device") {
			if _, err := bluetoothctl("disconnect", fields[1]); err != nil {
				g.logf("bluetooth: disconnect %s: %v", fields[1], err)
			}
		}
	}
}

func (g *Gateway) closeBluetoothWindow() {
	g.mu.Lock()
	g.btTimer = nil
	on := g.btMode
	g.mu.Unlock()
	if !on {
		return
	}
	if _, err := bluetoothctl("discoverable", "off"); err != nil {
		g.logf("bluetooth: discoverable off: %v", err)
	}
	if _, err := bluetoothctl("pairable", "off"); err != nil {
		g.logf("bluetooth: pairable off: %v", err)
	}
	g.logf("bluetooth: pairing window closed; paired devices keep their A2DP link")
	g.wifiReconnect()
}

func (g *Gateway) disableBluetooth() {
	g.mu.Lock()
	g.btMode = false
	if g.btTimer != nil {
		g.btTimer.Stop()
		g.btTimer = nil
	}
	g.mu.Unlock()
	if _, err := bluetoothctl("discoverable", "off"); err != nil {
		g.logf("bluetooth: discoverable off: %v", err)
	}
	if _, err := bluetoothctl("pairable", "off"); err != nil {
		g.logf("bluetooth: pairable off: %v", err)
	}
	g.disconnectBluetoothDevices()
	g.logf("bluetooth: pairing off")
	g.wifiReconnect()
}

func (g *Gateway) enableBluetooth() {
	ensureBluetoothAudio()
	if _, err := bluetoothctl("power", "on"); err != nil {
		g.logf("bluetooth: power on: %v", err)
	}
	if _, err := bluetoothctl("pairable", "on"); err != nil {
		g.logf("bluetooth: pairable on: %v", err)
	}
	if _, err := bluetoothctl("discoverable", "on"); err != nil {
		g.logf("bluetooth: discoverable on: %v", err)
		return
	}
	g.mu.Lock()
	g.btMode = true
	if g.btTimer != nil {
		g.btTimer.Stop()
	}
	g.btTimer = time.AfterFunc(bluetoothWindow, g.closeBluetoothWindow)
	g.mu.Unlock()
	g.logf("bluetooth: pairing on for %s (A2DP sink; bluealsad/bluealsa-aplay/bt-agent ready)", bluetoothWindow)
}

func (g *Gateway) toggleBluetooth() {
	g.mu.Lock()
	on := g.btMode
	g.mu.Unlock()
	if on {
		g.disableBluetooth()
		return
	}
	g.enableBluetooth()
}
