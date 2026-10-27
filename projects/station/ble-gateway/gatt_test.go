package main

import (
	"bytes"
	"encoding/binary"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/godbus/dbus/v5"
)

func TestParseInputEvent(t *testing.T) {
	buf := make([]byte, 16)
	binary.LittleEndian.PutUint32(buf[0:4], 123)
	binary.LittleEndian.PutUint32(buf[4:8], 456)
	binary.LittleEndian.PutUint16(buf[8:10], 1)
	binary.LittleEndian.PutUint16(buf[10:12], 113)
	binary.LittleEndian.PutUint32(buf[12:16], 0xffffffff)
	ev, ok := parseInputEvent(buf)
	if !ok {
		t.Fatal("parseInputEvent returned false")
	}
	if ev.Sec != 123 || ev.Usec != 456 || ev.Type != 1 || ev.Code != 113 || ev.Value != -1 {
		t.Fatalf("unexpected event: %+v", ev)
	}
	if _, ok := parseInputEvent(buf[:15]); ok {
		t.Fatal("short buffer accepted")
	}
}

func TestCharacteristicReadWriteOffsets(t *testing.T) {
	g := newTestGateway(t)
	c := newCharacteristic(g, 0, charSSID, "read", "write")
	if c.getName() != "ssid" {
		t.Fatalf("name = %q", c.getName())
	}
	props := c.getAll()
	if props["UUID"].Value().(string) != charSSID {
		t.Fatal("UUID prop missing")
	}
	if err := c.WriteValue([]byte("Home"), nil); err != nil {
		t.Fatal(err)
	}
	if err := c.WriteValue([]byte("WiFi"), map[string]dbus.Variant{"offset": dbus.MakeVariant(4)}); err != nil {
		t.Fatal(err)
	}
	if got := string(c.value); got != "HomeWiFi" {
		t.Fatalf("value = %q, want HomeWiFi", got)
	}
	got, err := c.ReadValue(map[string]dbus.Variant{"offset": dbus.MakeVariant(4)})
	if err != nil || string(got) != "WiFi" {
		t.Fatalf("ReadValue = %q, %v", got, err)
	}
	if _, err := c.ReadValue(map[string]dbus.Variant{"offset": dbus.MakeVariant(99)}); err == nil {
		t.Fatal("out-of-range read accepted")
	}
	if err := c.WriteValue([]byte("x"), map[string]dbus.Variant{"offset": dbus.MakeVariant(600)}); err == nil {
		t.Fatal("out-of-range write accepted")
	}
	if err := c.WriteValue([]byte("x"), map[string]dbus.Variant{"offset": dbus.MakeVariant(20)}); err == nil {
		t.Fatal("write past value accepted")
	}
}

func TestCharacteristicCommitAndNotify(t *testing.T) {
	g := newTestGateway(t)
	c := newCharacteristic(g, 4, charStatus, "read", "notify")
	if err := c.StartNotify(); err != nil {
		t.Fatal(err)
	}
	if !c.notifying {
		t.Fatal("notify not enabled")
	}
	if err := c.StopNotify(); err != nil {
		t.Fatal(err)
	}
	if c.notifying {
		t.Fatal("notify not disabled")
	}
	if err := c.WriteValue([]byte("x"), nil); err == nil {
		t.Fatal("status characteristic accepted write")
	}

	g.setStatus("IDLE")
	commit := newCharacteristic(g, 3, charCommit, "write")
	if err := commit.WriteValue([]byte("CANCEL"), nil); err != nil {
		t.Fatal(err)
	}
	if got := strings.TrimSpace(readFile(t, g.statusFile)); got != "IDLE" {
		t.Fatalf("status after CANCEL = %q", got)
	}
}

func readFile(t *testing.T, path string) string {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

func TestHandleCommitValidation(t *testing.T) {
	g := newTestGateway(t)
	for _, name := range []string{"ssid", "psk", "token"} {
		g.byName[name] = newCharacteristic(g, 0, charSSID)
	}
	g.handleCommit(nil)
	if got := strings.TrimSpace(readFile(t, g.statusFile)); got != "ERROR:EMPTY_COMMIT" {
		t.Fatalf("empty status = %q", got)
	}
	g.byName["token"].value = []byte("wrong")
	g.handleCommit([]byte("1"))
	if got := strings.TrimSpace(readFile(t, g.statusFile)); got != "ERROR:BAD_TOKEN" {
		t.Fatalf("bad token status = %q", got)
	}
	g.byName["token"].value = []byte(g.token)
	g.byName["ssid"].value = []byte("Home")
	g.byName["psk"].value = []byte("")
	g.handleCommit([]byte("1"))
	if got := strings.TrimSpace(readFile(t, g.statusFile)); got != "ERROR:EMPTY_PSK" {
		t.Fatalf("bad psk status = %q", got)
	}
}

func TestHandleCommitDryRun(t *testing.T) {
	g := newTestGateway(t)
	g.byName["ssid"] = newCharacteristic(g, 0, charSSID)
	g.byName["psk"] = newCharacteristic(g, 1, charPSK)
	g.byName["token"] = newCharacteristic(g, 2, charToken)
	g.byName["ssid"].value = []byte("Home")
	g.byName["psk"].value = []byte("password123")
	g.byName["token"].value = []byte(g.token)
	g.handleCommit([]byte("1"))
	deadline := time.Now().Add(2 * time.Second)
	for time.Now().Before(deadline) {
		if strings.TrimSpace(readFile(t, g.statusFile)) == "DRY_RUN_OK" {
			return
		}
		time.Sleep(10 * time.Millisecond)
	}
	t.Fatalf("status = %q, want DRY_RUN_OK", strings.TrimSpace(readFile(t, g.statusFile)))
}

func TestPropertiesAndObjects(t *testing.T) {
	g := newTestGateway(t)
	c := newCharacteristic(g, 0, charSSID, "read")
	c.value = []byte("abc")
	p := &Properties{getAll: c.getAll}
	if v, err := p.Get("x", "UUID"); err != nil || v.Value().(string) != charSSID {
		t.Fatalf("Get = %v, %v", v, err)
	}
	if _, err := p.Get("x", "missing"); err == nil {
		t.Fatal("missing property accepted")
	}
	if err := p.Set("x", "UUID", dbus.MakeVariant("y")); err == nil {
		t.Fatal("read-only property set accepted")
	}
	if all, err := p.GetAll("x"); err != nil || all["UUID"].Value().(string) != charSSID {
		t.Fatalf("GetAll = %v, %v", all, err)
	}

	svc := newService([]*Characteristic{c})
	if got := svc.props.getAll()["Characteristics"].Value().([]dbus.ObjectPath); len(got) != 1 {
		t.Fatalf("service chars = %v", got)
	}
	app := &Application{service: svc, chars: []*Characteristic{c}}
	objs, err := app.GetManagedObjects()
	if err != nil {
		t.Fatal(err)
	}
	if _, ok := objs[svcPath]["org.bluez.GattService1"]; !ok {
		t.Fatal("service object missing")
	}
	if _, ok := objs[c.path]["org.bluez.GattCharacteristic1"]; !ok {
		t.Fatal("characteristic object missing")
	}
	if err := g.enableBLE(); err == nil {
		t.Fatal("enableBLE without D-Bus should fail")
	}
}

func TestAdvertisementProperties(t *testing.T) {
	g := newTestGateway(t)
	a := newAdvertisement(g)
	props := a.props.getAll()
	if props["LocalName"].Value().(string) != "Station-Gateway" {
		t.Fatalf("LocalName = %v", props["LocalName"].Value())
	}
	if props["Type"].Value().(string) != "peripheral" {
		t.Fatalf("Type = %v", props["Type"].Value())
	}
	if err := a.Release(); err != nil {
		t.Fatal(err)
	}
}

func TestSetCharacteristicValueCopies(t *testing.T) {
	g := newTestGateway(t)
	c := newCharacteristic(g, 0, charSSID)
	src := []byte("abc")
	g.setCharacteristicValue(c, src)
	src[0] = 'x'
	if !bytes.Equal(c.value, []byte("abc")) {
		t.Fatalf("value was not copied: %q", c.value)
	}
}

func TestOpenInput(t *testing.T) {
	path := t.TempDir() + "/event"
	if err := os.WriteFile(path, []byte("x"), 0600); err != nil {
		t.Fatal(err)
	}
	f, err := openInput(path)
	if err != nil {
		t.Fatal(err)
	}
	f.Close()
}
