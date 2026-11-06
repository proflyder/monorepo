package catalogapi

// Golden vectors for GET /v1/catalog.
//
// The Go server is the reference: states.json is one /api/states payload,
// templates/*.txt are the answers to the discovery templates, and catalog.json
// pins the exact response body (status, headers and bytes) for the live
// catalog, the stale fallback, a scenario-label outage and the error paths.
// The Rust server reads the same files.
//
// Run `go test ./internal/catalogapi -run TestGoldenCatalog -update` to
// regenerate them after an intentional contract change.

import (
	"encoding/json"
	"flag"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/proflyder/monorepo/packages/go/ha"
	"github.com/proflyder/monorepo/solutions/ha-bridge/internal/catalog"
	"github.com/proflyder/monorepo/solutions/ha-bridge/internal/discovery"
)

var updateCatalogGolden = flag.Bool("update", false, "rewrite ha-control catalog golden vectors")

type goldenRules struct {
	ScenarioLabel string   `json:"scenarioLabel"`
	ProfileLabel  string   `json:"profileLabel"`
	Exclude       []string `json:"exclude"`
}

type goldenResponse struct {
	Status          int    `json:"status"`
	Body            string `json:"body,omitempty"`
	WWWAuthenticate string `json:"wwwAuthenticate,omitempty"`
}

type goldenCatalog struct {
	Rules     goldenRules               `json:"rules"`
	Responses map[string]goldenResponse `json:"responses"`
}

type goldenState struct {
	EntityID   string         `json:"entity_id"`
	State      string         `json:"state"`
	Attributes map[string]any `json:"attributes,omitempty"`
}

func goldenRulesValue() catalog.Rules {
	return catalog.Rules{
		ScenarioLabel: "scenario",
		ProfileLabel:  "voice_profile",
		Exclude:       []string{"switch.*_child_lock"},
	}
}

// goldenStates is a realistic /api/states payload: it covers every catalog
// branch, including Cyrillic names that a byte-wise truncation would split.
func goldenStates() []goldenState {
	longName := strings.Repeat("ж", 100)
	light := func(modes ...string) []string { return modes }
	return []goldenState{
		{
			EntityID: "switch.voice_profile", State: "off",
			Attributes: map[string]any{"friendly_name": "Профиль"},
		},
		{
			EntityID: "scene.morning", State: "2026-01-01T08:00:00+00:00",
			Attributes: map[string]any{"friendly_name": "Доброе утро", "icon": "mdi:weather-sunny"},
		},
		{
			EntityID: "scene.movie", State: "2026-01-01T00:00:00+00:00",
			Attributes: map[string]any{"friendly_name": "Кино", "icon": "mdi:movie"},
		},
		{
			// Infrastructure helper: an icon must never expose it.
			EntityID: "script.alarm_start", State: "off",
			Attributes: map[string]any{"friendly_name": "Alarm start", "icon": "mdi:alarm"},
		},
		{
			EntityID: "script.blackout", State: "off",
			Attributes: map[string]any{"friendly_name": "Blackout"},
		},
		{
			EntityID: "script.work_mode", State: "off",
			Attributes: map[string]any{"friendly_name": "Work mode", "icon": "mdi:laptop"},
		},
		{
			EntityID: "script.long_room", State: "off",
			Attributes: map[string]any{"friendly_name": longName},
		},
		{
			EntityID: "light.bedside_lamp", State: "on",
			Attributes: map[string]any{
				"friendly_name":         "Лампа у кровати",
				"icon":                  "mdi:lightbulb-outline",
				"supported_color_modes": []string{"color_temp", "hs"},
				"min_color_temp_kelvin": 1700,
				"max_color_temp_kelvin": 6500,
				"brightness":            128,
				"color_temp_kelvin":     3000,
				"hs_color":              []float64{30, 60},
				"color_mode":            "color_temp",
			},
		},
		{
			EntityID: "light.strip", State: "on",
			Attributes: map[string]any{
				"friendly_name":         "Подсветка",
				"supported_color_modes": []string{"hs"},
				"brightness":            255,
				"hs_color":              []float64{359.6, 50},
				"color_mode":            "hs",
			},
		},
		{
			EntityID: "light.hall", State: "off",
			Attributes: map[string]any{
				"friendly_name":         longName,
				"supported_color_modes": light("brightness"),
			},
		},
		{
			EntityID: "light.color_lamp", State: "on",
			Attributes: map[string]any{
				"friendly_name":         "Цветная лампа",
				"supported_color_modes": []string{"color_temp", "xy"},
				"min_color_temp_kelvin": 2000,
				"max_color_temp_kelvin": 6500,
				"brightness":            64,
				"color_temp_kelvin":     2700,
				"hs_color":              []float64{120, 80},
				"color_mode":            "xy",
			},
		},
		{
			EntityID: "light.onoff", State: "on",
			Attributes: map[string]any{
				"friendly_name":         "Розетка-лампа",
				"supported_color_modes": light("onoff"),
			},
		},
		{
			EntityID: "light.unknown", State: "unknown",
			Attributes: map[string]any{
				"friendly_name":         "Пропавшая лампа",
				"supported_color_modes": light("brightness"),
			},
		},
		{
			EntityID: "switch.desk_fan", State: "unavailable",
			Attributes: map[string]any{"friendly_name": "Вентилятор"},
		},
		{
			EntityID: "switch.router_child_lock", State: "off",
			Attributes: map[string]any{"friendly_name": "Child lock"},
		},
		{
			EntityID: "light.secret", State: "on",
			Attributes: map[string]any{"friendly_name": "Секрет"},
		},
		{
			EntityID: "sensor.temp", State: "21",
			Attributes: map[string]any{"friendly_name": "Температура"},
		},
		{
			// Long Cyrillic room name is truncated by characters, never by bytes.
			EntityID: "switch.long_room", State: "on",
			Attributes: map[string]any{"friendly_name": "Длинная комната"},
		},
	}
}

func goldenTemplates() map[string]string {
	return map[string]string{
		"areas": "switch.voice_profile|\n" +
			"scene.morning|Гостиная\n" +
			"scene.movie|Гостиная\n" +
			"script.blackout|\n" +
			"script.work_mode|Кабинет\n" +
			"script.long_room|" + strings.Repeat("к", 100) + "\n" +
			"light.bedside_lamp|Спальня\n" +
			"light.strip|Гостиная\n" +
			"light.hall|\n" +
			"light.color_lamp|Спальня\n" +
			"light.onoff|Кухня\n" +
			"switch.desk_fan|Кабинет\n" +
			"switch.long_room|" + strings.Repeat("к", 100) + "\n",
		"hidden":   "light.secret\n",
		"scenario": "script.blackout\nscript.work_mode\nscript.long_room\n",
		"profile":  "switch.voice_profile\n",
	}
}

// fakeHome serves the golden states and templates. Each template is chosen by
// the same markers the discovery provider puts into it.
type fakeHome struct {
	states    []byte
	templates map[string]string
	mu        sync.Mutex
	down      bool
	scenario  bool
}

func (f *fakeHome) set(fn func(*fakeHome)) {
	f.mu.Lock()
	defer f.mu.Unlock()
	fn(f)
}

func (f *fakeHome) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	f.mu.Lock()
	down, scenarioDown := f.down, f.scenario
	f.mu.Unlock()
	if down {
		http.Error(w, "down", http.StatusInternalServerError)
		return
	}
	switch {
	case r.Method == http.MethodGet && r.URL.Path == "/api/states":
		w.Header().Set("Content-Type", "application/json")
		_, _ = w.Write(f.states)
	case r.Method == http.MethodPost && r.URL.Path == "/api/template":
		var request struct {
			Template string `json:"template"`
		}
		if err := json.NewDecoder(r.Body).Decode(&request); err != nil {
			http.Error(w, "bad template", http.StatusBadRequest)
			return
		}
		switch {
		case strings.Contains(request.Template, "area_name"):
			_, _ = w.Write([]byte(f.templates["areas"]))
		case strings.Contains(request.Template, "is_hidden_entity"):
			_, _ = w.Write([]byte(f.templates["hidden"]))
		case strings.Contains(request.Template, "label_entities('"+goldenRulesValue().ScenarioLabel+"')"):
			if scenarioDown {
				http.Error(w, "labels unavailable", http.StatusInternalServerError)
				return
			}
			_, _ = w.Write([]byte(f.templates["scenario"]))
		case strings.Contains(request.Template, "label_entities('"+goldenRulesValue().ProfileLabel+"')"):
			_, _ = w.Write([]byte(f.templates["profile"]))
		default:
			http.Error(w, "unexpected template", http.StatusBadRequest)
		}
	default:
		http.Error(w, "not found", http.StatusNotFound)
	}
}

func goldenProvider(t *testing.T, server *httptest.Server, ttl time.Duration) *discovery.Provider {
	t.Helper()
	client := ha.New(server.URL, "secret", server.Client())
	return discovery.New(client, goldenRulesValue(), ttl, nil)
}

func capture(t *testing.T, handler http.Handler, auth string) goldenResponse {
	t.Helper()
	request := httptest.NewRequest(http.MethodGet, "/v1/catalog", nil)
	if auth != "" {
		request.Header.Set("Authorization", auth)
	}
	recorder := httptest.NewRecorder()
	handler.ServeHTTP(recorder, request)
	return goldenResponse{
		Status:          recorder.Code,
		Body:            recorder.Body.String(),
		WWWAuthenticate: recorder.Header().Get("WWW-Authenticate"),
	}
}

func goldenCatalogFixture(t *testing.T) (goldenCatalog, []byte, map[string]string) {
	t.Helper()
	states, err := json.MarshalIndent(goldenStates(), "", "  ")
	if err != nil {
		t.Fatalf("encode states: %v", err)
	}
	states = append(states, '\n')
	templates := goldenTemplates()
	fake := &fakeHome{states: states, templates: templates}
	server := httptest.NewServer(fake)
	t.Cleanup(server.Close)

	responses := map[string]goldenResponse{}

	// Live catalog, cached for the whole test.
	live := goldenProvider(t, server, time.Hour)
	handler := Handler(live, "control-secret", nil)
	responses["live"] = capture(t, handler, "Bearer control-secret")
	responses["unauthorized"] = capture(t, handler, "")

	// The last good catalog is served when Home Assistant stops answering.
	stale := goldenProvider(t, server, 0)
	staleHandler := Handler(stale, "control-secret", nil)
	_ = capture(t, staleHandler, "Bearer control-secret")
	fake.set(func(f *fakeHome) { f.down = true })
	responses["stale"] = capture(t, staleHandler, "Bearer control-secret")
	fake.set(func(f *fakeHome) { f.down = false })

	// The scenario label decides which scripts are exposed: when it cannot be
	// read, no script is exposed (fail closed); devices and scenes stay.
	outage := goldenProvider(t, server, 0)
	outageHandler := Handler(outage, "control-secret", nil)
	fake.set(func(f *fakeHome) { f.scenario = true })
	responses["scenarioLabelOutage"] = capture(t, outageHandler, "Bearer control-secret")
	fake.set(func(f *fakeHome) { f.scenario = false })

	// Nothing discovered yet and Home Assistant is down: 502, no inventory.
	unreachable := goldenProvider(t, server, 0)
	fake.set(func(f *fakeHome) { f.down = true })
	responses["unreachable"] = capture(t, Handler(unreachable, "control-secret", nil), "Bearer control-secret")
	fake.set(func(f *fakeHome) { f.down = false })

	if responses["live"].Status != http.StatusOK {
		t.Fatalf("live catalog status %d: %s", responses["live"].Status, responses["live"].Body)
	}
	return goldenCatalog{
		Rules: goldenRules{
			ScenarioLabel: goldenRulesValue().ScenarioLabel,
			ProfileLabel:  goldenRulesValue().ProfileLabel,
			Exclude:       goldenRulesValue().Exclude,
		},
		Responses: responses,
	}, states, templates
}

func catalogGoldenDir(t *testing.T) string {
	t.Helper()
	if root := os.Getenv("TEST_SRCDIR"); root != "" {
		workspace := os.Getenv("TEST_WORKSPACE")
		for _, dir := range []string{
			filepath.Join(root, workspace, "solutions/ha-bridge/testdata/catalog"),
			filepath.Join(root, "solutions/ha-bridge/testdata/catalog"),
		} {
			if info, err := os.Stat(dir); err == nil && info.IsDir() {
				return dir
			}
		}
	}
	candidates := []string{"../../testdata/catalog", "solutions/ha-bridge/testdata/catalog"}
	for _, dir := range candidates {
		if info, err := os.Stat(dir); err == nil && info.IsDir() {
			return dir
		}
	}
	return candidates[0]
}

func TestGoldenCatalog(t *testing.T) {
	fixture, states, templates := goldenCatalogFixture(t)
	dir := catalogGoldenDir(t)

	if *updateCatalogGolden {
		if err := os.MkdirAll(filepath.Join(dir, "templates"), 0o755); err != nil {
			t.Fatalf("create templates: %v", err)
		}
		if err := os.WriteFile(filepath.Join(dir, "states.json"), states, 0o644); err != nil {
			t.Fatalf("write states: %v", err)
		}
		for name, body := range templates {
			if err := os.WriteFile(filepath.Join(dir, "templates", name+".txt"), []byte(body), 0o644); err != nil {
				t.Fatalf("write template %s: %v", name, err)
			}
		}
		encoded, err := json.MarshalIndent(fixture, "", "  ")
		if err != nil {
			t.Fatalf("encode catalog: %v", err)
		}
		encoded = append(encoded, '\n')
		if err := os.WriteFile(filepath.Join(dir, "catalog.json"), encoded, 0o644); err != nil {
			t.Fatalf("write catalog: %v", err)
		}
		return
	}

	wantStates, err := os.ReadFile(filepath.Join(dir, "states.json"))
	if err != nil {
		t.Fatalf("read states (run go test -run TestGoldenCatalog -update): %v", err)
	}
	if string(wantStates) != string(states) {
		t.Errorf("states.json does not match the Go fixture; run go test -run TestGoldenCatalog -update")
	}
	for name, body := range templates {
		want, err := os.ReadFile(filepath.Join(dir, "templates", name+".txt"))
		if err != nil {
			t.Fatalf("read template %s: %v", name, err)
		}
		if string(want) != body {
			t.Errorf("templates/%s.txt does not match the Go fixture", name)
		}
	}
	want, err := os.ReadFile(filepath.Join(dir, "catalog.json"))
	if err != nil {
		t.Fatalf("read catalog: %v", err)
	}
	encoded, err := json.MarshalIndent(fixture, "", "  ")
	if err != nil {
		t.Fatalf("encode catalog: %v", err)
	}
	encoded = append(encoded, '\n')
	if string(want) != string(encoded) {
		t.Errorf("catalog.json does not match the Go server; run go test -run TestGoldenCatalog -update\nwant:\n%s\ngot:\n%s", encoded, want)
	}
}
