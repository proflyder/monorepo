package main

import (
	"encoding/binary"
	"fmt"
	"log"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/godbus/dbus/v5"
)

const (
	serviceUUID = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c5d"
	charSSID    = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c01"
	charPSK     = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c02"
	charToken   = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c03"
	charCommit  = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c04"
	charStatus  = "a1b2c3d4-e5f6-4a7b-8c9d-0e1f2a3b4c05"

	appPath  = dbus.ObjectPath("/com/station/gateway")
	advPath  = dbus.ObjectPath("/com/station/gateway/advertisement")
	svcPath  = dbus.ObjectPath("/com/station/gateway/service0")
	charBase = "/com/station/gateway/service0/char"
)

// Properties is a small generic implementation of org.freedesktop.DBus.Properties.
type Properties struct {
	getAll func() map[string]dbus.Variant
	set    func(name string, v dbus.Variant) *dbus.Error
}

func (p *Properties) Get(iface, name string) (dbus.Variant, *dbus.Error) {
	m := p.getAll()
	v, ok := m[name]
	if !ok {
		return dbus.MakeVariant(""), &dbus.Error{Name: "org.freedesktop.DBus.Error.InvalidArgs", Body: []interface{}{"unknown property " + name}}
	}
	return v, nil
}

func (p *Properties) GetAll(iface string) (map[string]dbus.Variant, *dbus.Error) {
	return p.getAll(), nil
}

func (p *Properties) Set(iface, name string, v dbus.Variant) *dbus.Error {
	if p.set == nil {
		return &dbus.Error{Name: "org.freedesktop.DBus.Error.PropertyReadOnly", Body: []interface{}{name}}
	}
	return p.set(name, v)
}

type Characteristic struct {
	path    dbus.ObjectPath
	uuid    string
	flags   []string
	gateway *Gateway

	mu        sync.Mutex
	value     []byte
	notifying bool
	props     *Properties
}

func newCharacteristic(g *Gateway, idx int, uuid string, flags ...string) *Characteristic {
	c := &Characteristic{
		path:    dbus.ObjectPath(fmt.Sprintf("%s%d", charBase, idx)),
		uuid:    uuid,
		flags:   flags,
		gateway: g,
	}
	c.props = &Properties{getAll: c.getAll}
	return c
}

func (c *Characteristic) getName() string {
	switch c.uuid {
	case charSSID:
		return "ssid"
	case charPSK:
		return "psk"
	case charToken:
		return "token"
	case charCommit:
		return "commit"
	case charStatus:
		return "status"
	}
	return "unknown"
}

func (c *Characteristic) getAll() map[string]dbus.Variant {
	c.mu.Lock()
	defer c.mu.Unlock()
	return map[string]dbus.Variant{
		"UUID":        dbus.MakeVariant(c.uuid),
		"Service":     dbus.MakeVariant(svcPath),
		"Flags":       dbus.MakeVariant(c.flags),
		"Value":       dbus.MakeVariant(c.valueCopyLocked()),
		"Notifying":   dbus.MakeVariant(c.notifying),
		"Descriptors": dbus.MakeVariant([]dbus.ObjectPath{}),
	}
}

func (c *Characteristic) valueCopyLocked() []byte {
	out := make([]byte, len(c.value))
	copy(out, c.value)
	return out
}

func (c *Characteristic) ReadValue(options map[string]dbus.Variant) ([]byte, *dbus.Error) {
	c.mu.Lock()
	defer c.mu.Unlock()
	offset := 0
	if v, ok := options["offset"]; ok {
		_ = v.Store(&offset)
	}
	if offset < 0 || offset > len(c.value) {
		return nil, &dbus.Error{Name: "org.bluez.Error.InvalidOffset"}
	}
	out := make([]byte, len(c.value)-offset)
	copy(out, c.value[offset:])
	return out, nil
}

func (c *Characteristic) WriteValue(value []byte, options map[string]dbus.Variant) *dbus.Error {
	offset := 0
	if v, ok := options["offset"]; ok {
		_ = v.Store(&offset)
	}
	if offset < 0 || offset > 512 {
		return &dbus.Error{Name: "org.bluez.Error.InvalidOffset"}
	}

	switch c.uuid {
	case charSSID, charPSK, charToken:
		c.mu.Lock()
		if offset == 0 {
			c.value = append([]byte(nil), value...)
		} else {
			if offset > len(c.value) {
				c.mu.Unlock()
				return &dbus.Error{Name: "org.bluez.Error.InvalidOffset"}
			}
			c.value = append(c.value[:offset], value...)
		}
		c.mu.Unlock()
		c.gateway.logf("write %s: %d bytes (offset %d)", c.getName(), len(value), offset)
		return nil
	case charCommit:
		c.gateway.handleCommit(value)
		return nil
	default:
		return &dbus.Error{Name: "org.bluez.Error.NotPermitted"}
	}
}

func (c *Characteristic) StartNotify() *dbus.Error {
	c.mu.Lock()
	c.notifying = true
	v := c.valueCopyLocked()
	c.mu.Unlock()
	c.gateway.logf("notify enabled for %s", c.getName())
	c.gateway.emitValue(c, v)
	return nil
}

func (c *Characteristic) StopNotify() *dbus.Error {
	c.mu.Lock()
	c.notifying = false
	c.mu.Unlock()
	return nil
}

func (g *Gateway) setCharacteristicValue(c *Characteristic, v []byte) {
	c.mu.Lock()
	c.value = append([]byte(nil), v...)
	notifying := c.notifying
	c.mu.Unlock()
	if notifying {
		g.emitValue(c, v)
	}
}

func (g *Gateway) emitValue(c *Characteristic, v []byte) {
	if g.conn == nil {
		return
	}
	changed := map[string]dbus.Variant{"Value": dbus.MakeVariant(v)}
	if err := g.conn.Emit(c.path, "org.freedesktop.DBus.Properties.PropertiesChanged",
		"org.bluez.GattCharacteristic1", changed, []string{}); err != nil {
		g.logf("emit PropertiesChanged: %v", err)
	}
}

type Service struct {
	chars []*Characteristic
	props *Properties
}

func newService(chars []*Characteristic) *Service {
	s := &Service{chars: chars}
	s.props = &Properties{getAll: func() map[string]dbus.Variant {
		paths := make([]dbus.ObjectPath, 0, len(chars))
		for _, c := range chars {
			paths = append(paths, c.path)
		}
		return map[string]dbus.Variant{
			"UUID":            dbus.MakeVariant(serviceUUID),
			"Primary":         dbus.MakeVariant(true),
			"Characteristics": dbus.MakeVariant(paths),
		}
	}}
	return s
}

type Advertisement struct {
	g     *Gateway
	props *Properties
}

func newAdvertisement(g *Gateway) *Advertisement {
	a := &Advertisement{g: g}
	a.props = &Properties{getAll: func() map[string]dbus.Variant {
		return map[string]dbus.Variant{
			"Type":         dbus.MakeVariant("peripheral"),
			"ServiceUUIDs": dbus.MakeVariant([]string{serviceUUID}),
			"LocalName":    dbus.MakeVariant("Station-Gateway"),
			"Includes":     dbus.MakeVariant([]string{"tx-power"}),
		}
	}}
	return a
}

func (a *Advertisement) Release() *dbus.Error {
	a.g.logf("advertisement released by BlueZ")
	return nil
}

type Application struct {
	service *Service
	chars   []*Characteristic
}

func (a *Application) GetManagedObjects() (map[dbus.ObjectPath]map[string]map[string]dbus.Variant, *dbus.Error) {
	out := make(map[dbus.ObjectPath]map[string]map[string]dbus.Variant)
	out[appPath] = map[string]map[string]dbus.Variant{
		"org.freedesktop.DBus.ObjectManager": {},
	}
	out[svcPath] = map[string]map[string]dbus.Variant{
		"org.bluez.GattService1": a.service.props.getAll(),
	}
	for _, c := range a.chars {
		out[c.path] = map[string]map[string]dbus.Variant{
			"org.bluez.GattCharacteristic1": c.props.getAll(),
		}
	}
	return out, nil
}

func (g *Gateway) registerGATT() error {
	chars := []*Characteristic{
		newCharacteristic(g, 0, charSSID, "read", "write"),
		newCharacteristic(g, 1, charPSK, "write"),
		newCharacteristic(g, 2, charToken, "write"),
		newCharacteristic(g, 3, charCommit, "write"),
		newCharacteristic(g, 4, charStatus, "read", "notify"),
	}
	g.chars = chars
	for i, c := range chars {
		g.byName[c.getName()] = c
		if err := g.conn.Export(c, c.path, "org.bluez.GattCharacteristic1"); err != nil {
			return fmt.Errorf("export char %d: %w", i, err)
		}
		if err := g.conn.Export(c.props, c.path, "org.freedesktop.DBus.Properties"); err != nil {
			return fmt.Errorf("export char props %d: %w", i, err)
		}
	}
	svc := newService(chars)
	if err := g.conn.Export(svc, svcPath, "org.bluez.GattService1"); err != nil {
		return fmt.Errorf("export service: %w", err)
	}
	if err := g.conn.Export(svc.props, svcPath, "org.freedesktop.DBus.Properties"); err != nil {
		return fmt.Errorf("export service props: %w", err)
	}
	app := &Application{service: svc, chars: chars}
	if err := g.conn.Export(app, appPath, "org.freedesktop.DBus.ObjectManager"); err != nil {
		return fmt.Errorf("export object manager: %w", err)
	}
	obj := g.conn.Object("org.bluez", "/org/bluez/hci0")
	if err := obj.Call("org.bluez.GattManager1.RegisterApplication", 0, appPath, map[string]dbus.Variant{}).Err; err != nil {
		return fmt.Errorf("RegisterApplication: %w", err)
	}
	g.logf("GATT application registered")
	return nil
}

func (g *Gateway) unregisterGATT() {
	obj := g.conn.Object("org.bluez", "/org/bluez/hci0")
	if err := obj.Call("org.bluez.GattManager1.UnregisterApplication", 0, appPath).Err; err != nil {
		g.logf("UnregisterApplication: %v", err)
	}
}

func (g *Gateway) registerAdvertisement() error {
	adv := newAdvertisement(g)
	g.adv = adv
	if err := g.conn.Export(adv, advPath, "org.bluez.LEAdvertisement1"); err != nil {
		return fmt.Errorf("export advertisement: %w", err)
	}
	if err := g.conn.Export(adv.props, advPath, "org.freedesktop.DBus.Properties"); err != nil {
		return fmt.Errorf("export advertisement props: %w", err)
	}
	obj := g.conn.Object("org.bluez", "/org/bluez/hci0")
	if err := obj.Call("org.bluez.LEAdvertisingManager1.RegisterAdvertisement", 0, advPath, map[string]dbus.Variant{}).Err; err != nil {
		return fmt.Errorf("RegisterAdvertisement: %w", err)
	}
	g.logf("advertisement registered")
	return nil
}

func (g *Gateway) unregisterAdvertisement() {
	if g.adv == nil {
		return
	}
	obj := g.conn.Object("org.bluez", "/org/bluez/hci0")
	if err := obj.Call("org.bluez.LEAdvertisingManager1.UnregisterAdvertisement", 0, advPath).Err; err != nil {
		g.logf("UnregisterAdvertisement: %v", err)
	}
	g.adv = nil
}

func (g *Gateway) setAdapterConnectable(v bool) {
	obj := g.conn.Object("org.bluez", "/org/bluez/hci0")
	err := obj.Call("org.freedesktop.DBus.Properties.Set", 0,
		"org.bluez.Adapter1", "Connectable", dbus.MakeVariant(v)).Err
	if err != nil {
		g.logf("set Connectable=%v: %v", v, err)
	}
}

func (g *Gateway) advertiseEnabled() bool {
	g.mu.Lock()
	defer g.mu.Unlock()
	return g.advertising
}

func (g *Gateway) enableBLE() error {
	g.mu.Lock()
	if g.advertising {
		g.mu.Unlock()
		return nil
	}
	g.mu.Unlock()

	if g.conn == nil {
		return fmt.Errorf("no dbus connection")
	}
	if !g.gattRegistered {
		if err := g.registerGATT(); err != nil {
			return err
		}
		g.gattRegistered = true
	}
	g.setAdapterConnectable(true)
	if err := g.registerAdvertisement(); err != nil {
		return err
	}
	g.mu.Lock()
	g.advertising = true
	g.mu.Unlock()
	g.logf("BLE advertising ON")
	g.setStatus("READY")
	return nil
}

func (g *Gateway) disableBLE() {
	g.mu.Lock()
	if !g.advertising {
		g.mu.Unlock()
		return
	}
	g.advertising = false
	g.mu.Unlock()

	g.unregisterAdvertisement()
	if g.gattRegistered {
		g.unregisterGATT()
		g.gattRegistered = false
	}
	g.logf("BLE advertising OFF")
	g.setStatus("IDLE")
}

// input_event layout on 32-bit armhf Linux: timeval (int32,int32), type u16, code u16, value i32.
type inputEvent struct {
	Sec   int32
	Usec  int32
	Type  uint16
	Code  uint16
	Value int32
}

func parseInputEvent(buf []byte) (inputEvent, bool) {
	if len(buf) < 16 {
		return inputEvent{}, false
	}
	return inputEvent{
		Sec:   int32(binary.LittleEndian.Uint32(buf[0:4])),
		Usec:  int32(binary.LittleEndian.Uint32(buf[4:8])),
		Type:  binary.LittleEndian.Uint16(buf[8:10]),
		Code:  binary.LittleEndian.Uint16(buf[10:12]),
		Value: int32(binary.LittleEndian.Uint32(buf[12:16])),
	}, true
}

func (g *Gateway) runButton(path string) {
	f, err := openInput(path)
	if err != nil {
		g.logf("button: %v (button disabled)", err)
		return
	}
	defer f.Close()
	g.logf("button monitor: %s (passive, no EVIOCGRAB)", path)

	var (
		mu            sync.Mutex
		pressAt       time.Time
		presses       []time.Time
		holdFired     bool
		holdTimer     *time.Timer
		decisionTimer *time.Timer
	)

	// Evaluate a group of short presses only after the user has stopped. This
	// lets 3 presses open the Wi-Fi gateway and 4 presses toggle Bluetooth,
	// without the third press of a four-press gesture firing too early.
	decide := func() {
		mu.Lock()
		n := len(presses)
		presses = nil
		mu.Unlock()
		switch n {
		case 1:
			g.logf("button: 1 short press (hardware mute is handled by Pulsar)")
		case 3:
			g.logf("button: 3 short presses -> toggle Wi-Fi gateway")
			g.toggleManual()
		case 4:
			g.logf("button: 4 short presses -> toggle Bluetooth")
			g.toggleBluetooth()
		default:
			if n > 0 {
				g.logf("button: %d short presses (ignored)", n)
			}
		}
	}
	scheduleDecision := func() {
		mu.Lock()
		if decisionTimer != nil {
			decisionTimer.Stop()
		}
		decisionTimer = time.AfterFunc(1500*time.Millisecond, decide)
		mu.Unlock()
	}

	buf := make([]byte, 16)
	for {
		n, err := f.Read(buf)
		if err != nil {
			g.logf("button read: %v", err)
			time.Sleep(time.Second)
			continue
		}
		if n != 16 {
			continue
		}
		ev, ok := parseInputEvent(buf)
		if !ok {
			continue
		}
		switch ev.Value {
		case 1:
			mu.Lock()
			if decisionTimer != nil {
				decisionTimer.Stop()
			}
			pressAt = time.Now()
			holdFired = false
			if holdTimer != nil {
				holdTimer.Stop()
			}
			holdTimer = time.AfterFunc(5*time.Second, func() {
				mu.Lock()
				if pressAt.IsZero() {
					mu.Unlock()
					return
				}
				holdFired = true
				mu.Unlock()
				g.logf("button: hold 5s -> open Wi-Fi setup window")
				g.openManual()
			})
			mu.Unlock()
			g.logf("button: KEY_MUTE down")
		case 0:
			mu.Lock()
			if pressAt.IsZero() {
				mu.Unlock()
				continue
			}
			d := time.Since(pressAt)
			pressAt = time.Time{}
			if holdTimer != nil {
				holdTimer.Stop()
				holdTimer = nil
			}
			fired := holdFired
			holdFired = false
			mu.Unlock()
			g.logf("button: KEY_MUTE up (%s)", d)
			if fired {
				mu.Lock()
				presses = nil
				if decisionTimer != nil {
					decisionTimer.Stop()
				}
				mu.Unlock()
				continue
			}
			if d > 400*time.Millisecond {
				mu.Lock()
				presses = nil
				mu.Unlock()
				continue
			}
			now := time.Now()
			mu.Lock()
			presses = append(presses, now)
			cut := now.Add(-1500 * time.Millisecond)
			for len(presses) > 0 && presses[0].Before(cut) {
				presses = presses[1:]
			}
			mu.Unlock()
			scheduleDecision()
		}
	}
}

func (g *Gateway) characteristicString(name string) string {
	c := g.byName[name]
	if c == nil {
		return ""
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	return string(c.value)
}

func (g *Gateway) handleCommit(value []byte) {
	cmd := strings.TrimSpace(string(value))
	g.logf("commit request: %q", cmd)
	if cmd == "" {
		g.setStatus("ERROR:EMPTY_COMMIT")
		return
	}
	if cmd == "CANCEL" {
		g.disableBLE()
		return
	}
	ssid := strings.TrimSpace(g.characteristicString("ssid"))
	psk := strings.TrimSpace(g.characteristicString("psk"))
	token := strings.TrimSpace(g.characteristicString("token"))
	if token != g.token {
		g.logf("commit: bad token")
		g.setStatus("ERROR:BAD_TOKEN")
		return
	}
	if err := validateWiFi(ssid, psk); err != nil {
		g.logf("commit: invalid credentials: %v", err)
		g.setStatus("ERROR:" + err.Error())
		return
	}
	g.mu.Lock()
	g.ssid = ssid
	g.psk = psk
	g.mu.Unlock()
	g.setStatus("CONNECTING")
	go g.applyWiFi(ssid, psk)
}

func (g *Gateway) toggleManual() {
	g.mu.Lock()
	now := time.Now()
	if g.manualUntil.After(now) {
		g.manualUntil = time.Time{}
		g.mu.Unlock()
		g.logf("manual window closed")
		g.disableBLE()
		return
	}
	g.manualUntil = now.Add(15 * time.Minute)
	g.mu.Unlock()
	g.logf("manual window opened for 15 minutes")
	if err := g.enableBLE(); err != nil {
		g.logf("enable BLE: %v", err)
	}
}

func (g *Gateway) openManual() {
	g.mu.Lock()
	g.manualUntil = time.Now().Add(15 * time.Minute)
	g.mu.Unlock()
	g.logf("manual window opened for 15 minutes")
	if err := g.enableBLE(); err != nil {
		g.logf("enable BLE: %v", err)
	}
}

func (g *Gateway) closeManual() {
	g.mu.Lock()
	g.manualUntil = time.Time{}
	g.mu.Unlock()
	g.disableBLE()
	g.logf("manual window closed")
}

func (g *Gateway) setStatus(s string) {
	g.mu.Lock()
	g.status = s
	statusFile := g.statusFile
	g.mu.Unlock()
	g.logf("status: %s", s)
	if statusFile != "" {
		_ = os.WriteFile(statusFile, []byte(s+"\n"), 0644)
	}
	if c, ok := g.byName["status"]; ok {
		g.setCharacteristicValue(c, []byte(s))
	}
}

func (g *Gateway) logf(format string, args ...interface{}) {
	if g.logger != nil {
		g.logger.Printf(format, args...)
	} else {
		log.Printf(format, args...)
	}
}
