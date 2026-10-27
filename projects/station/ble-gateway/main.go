package main

import (
	"crypto/rand"
	"flag"
	"fmt"
	"log"
	"math/big"
	"os"
	"os/signal"
	"path/filepath"
	"sync"
	"syscall"
	"time"

	"github.com/godbus/dbus/v5"
)

type Gateway struct {
	conn   *dbus.Conn
	logger *log.Logger

	mu             sync.Mutex
	byName         map[string]*Characteristic
	chars          []*Characteristic
	adv            *Advertisement
	gattRegistered bool
	advertising    bool

	token     string
	tokenFile string

	status      string
	statusFile  string
	manualUntil time.Time

	btMode           bool
	btTimer          *time.Timer
	controlToken     string
	controlTokenFile string

	ssid     string
	psk      string
	applying bool

	dryRun   bool
	autoWiFi bool
	button   string

	disconnectedSince time.Time
}

func main() {
	var (
		logPath          = flag.String("log", "", "log file")
		tokenFile        = flag.String("token-file", "/data/quasar/ble-gateway/token", "token file")
		buttonPath       = flag.String("button", "/dev/input/event2", "input event device")
		dryRun           = flag.Bool("dry-run", false, "do not apply Wi-Fi changes")
		autoWiFi         = flag.Bool("auto", true, "advertise automatically when Wi-Fi is down")
		noButton         = flag.Bool("no-button", false, "disable button monitor")
		force            = flag.Bool("force", false, "enable BLE immediately for testing")
		testWPA          = flag.Bool("test-wpa", false, "add/set/remove a temporary wpa network and exit")
		controlAddr      = flag.String("control-addr", "0.0.0.0:8099", "HTTP control listen address (empty disables)")
		controlTokenFile = flag.String("control-token-file", "/data/quasar/ble-gateway/control-token", "HTTP control token file")
	)
	flag.Parse()

	if *logPath != "" {
		_ = os.MkdirAll(filepath.Dir(*logPath), 0755)
		f, err := os.OpenFile(*logPath, os.O_CREATE|os.O_WRONLY|os.O_APPEND, 0644)
		if err != nil {
			log.Fatalf("log: %v", err)
		}
		defer f.Close()
		log.SetOutput(f)
	}
	log.SetFlags(log.LstdFlags | log.Lmicroseconds)

	conn, err := dbus.SystemBus()
	if err != nil {
		log.Fatalf("system bus: %v", err)
	}
	defer conn.Close()
	if _, err := conn.RequestName("com.station.gateway", dbus.NameFlagDoNotQueue); err != nil {
		log.Printf("RequestName: %v", err)
	}
	log.Printf("dbus names: %v", conn.Names())

	g := &Gateway{
		conn:             conn,
		logger:           log.Default(),
		byName:           make(map[string]*Characteristic),
		tokenFile:        *tokenFile,
		statusFile:       filepath.Join(filepath.Dir(*tokenFile), "status"),
		status:           "IDLE",
		dryRun:           *dryRun,
		autoWiFi:         *autoWiFi,
		button:           *buttonPath,
		controlTokenFile: *controlTokenFile,
	}
	if err := g.loadToken(); err != nil {
		log.Fatalf("token: %v", err)
	}
	if *testWPA {
		if err := testWPASet(); err != nil {
			log.Fatalf("test-wpa: %v", err)
		}
		return
	}
	log.Printf("station BLE gateway starting; token file %s; dry-run=%v auto=%v", g.tokenFile, g.dryRun, g.autoWiFi)
	if *controlAddr != "" {
		if err := g.startControl(*controlAddr, *controlTokenFile); err != nil {
			log.Printf("control HTTP disabled: %v", err)
		}
	}

	if !*noButton {
		go g.runButton(g.button)
	}
	go g.autoLoop()

	// Idle until a phone is needed. 3 short presses or auto-no-WiFi enables BLE.
	g.setStatus("IDLE")
	if *force {
		if err := g.enableBLE(); err != nil {
			log.Printf("force enable BLE: %v", err)
		}
	}

	sigc := make(chan os.Signal, 1)
	signal.Notify(sigc, syscall.SIGTERM, syscall.SIGINT)
	go func() {
		<-sigc
		log.Printf("shutdown signal; unregistering BLE")
		g.disableBLE()
		_ = conn.Close()
		os.Exit(0)
	}()
	select {}
}

func (g *Gateway) loadToken() error {
	if b, err := os.ReadFile(g.tokenFile); err == nil {
		g.token = trimToken(string(b))
		if g.token != "" {
			g.logf("loaded token from %s", g.tokenFile)
			return nil
		}
	}
	// Generate a default 6 digit token. An existing file may contain 6-12 digits.
	// to the log to avoid accidental disclosure.
	n, err := rand.Int(rand.Reader, big.NewInt(900000))
	if err != nil {
		return err
	}
	g.token = fmt.Sprintf("%06d", n.Int64()+100000)
	if err := os.MkdirAll(filepath.Dir(g.tokenFile), 0700); err != nil {
		return err
	}
	if err := os.WriteFile(g.tokenFile, []byte(g.token+"\n"), 0600); err != nil {
		return err
	}
	g.logf("generated token in %s", g.tokenFile)
	return nil
}

func trimToken(s string) string {
	out := make([]rune, 0, len(s))
	for _, r := range s {
		if r >= '0' && r <= '9' {
			out = append(out, r)
		}
	}
	if len(out) > 12 {
		out = out[:12]
	}
	return string(out)
}

func (g *Gateway) autoLoop() {
	ticker := time.NewTicker(5 * time.Second)
	defer ticker.Stop()
	for range ticker.C {
		if !g.autoWiFi {
			continue
		}
		g.mu.Lock()
		applying := g.applying
		manual := g.manualUntil.After(time.Now())
		g.mu.Unlock()
		if applying {
			continue
		}
		state := wpaState()
		if state == "COMPLETED" {
			g.disconnectedSince = time.Time{}
			if g.advertiseEnabled() && !manual {
				g.logf("Wi-Fi connected; stopping automatic advertisement")
				g.disableBLE()
			}
			continue
		}
		if g.disconnectedSince.IsZero() {
			g.disconnectedSince = time.Now()
			g.logf("Wi-Fi state %q; auto timer started", state)
			continue
		}
		if time.Since(g.disconnectedSince) >= 20*time.Second && !g.advertiseEnabled() && !manual {
			g.logf("Wi-Fi down for 20s; starting automatic BLE gateway")
			if err := g.enableBLE(); err != nil {
				g.logf("auto enable BLE: %v", err)
			}
		}
	}
}
