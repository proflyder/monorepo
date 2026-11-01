package catalog

import (
	"os"
	"strings"
	"testing"
)

const minimalCatalog = `
overview: |
  ## Топология
  Один кластер.
ignore_alerts: [Watchdog, InfoInhibitor]
services:
  - id: scheduler
    aliases:
      ru: [планировщик, будильники]
      en: [scheduler, alarms]
    kind: k8s
    namespace: home
    workload:
      kind: Deployment
      name: scheduler
    containers: [scheduler]
    signals:
      - promql: up{job="scheduler"} == 0
        meaning: планировщик недоступен
        status: true
    logs:
      source: pods
      containers: [scheduler]
    restart:
      allowed: true
      warning: Будильники в ближайшие минуты могут не сработать.
    description: Планировщик будильников.
  - id: hermes
    aliases:
      ru: [гермес, ассистент]
      en: [hermes]
    kind: k8s
    namespace: home
    workload:
      kind: Deployment
      name: hermes
    signals:
      - promql: ricardo_endpoint_up{endpoint="hermes"} == 0
        meaning: Гермес недоступен
    logs:
      source: loki
      selector: '{namespace="home"}'
    restart:
      allowed: true
      self_disrupting: true
      warning: Текущий разговор прервётся.
  - id: ha
    aliases:
      ru: [дом, хоум ассистент]
      en: [ha]
    kind: external
    signals:
      - promql: ricardo_endpoint_up{endpoint="ha"} == 0
        meaning: Home Assistant недоступен
        status: true
    restart:
      allowed: false
`

func TestParseValid(t *testing.T) {
	cat, err := Parse([]byte(minimalCatalog))
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	if len(cat.Services) != 3 {
		t.Fatalf("services = %d, want 3", len(cat.Services))
	}
	if len(cat.K8sServices()) != 2 {
		t.Fatalf("k8s services = %d, want 2", len(cat.K8sServices()))
	}
	if got := cat.OverviewText(); !strings.Contains(got, "Топология") {
		t.Fatalf("overview = %q", got)
	}
	text := cat.CatalogText()
	for _, want := range []string{"## scheduler", "restart: allowed", "workload: Deployment home/scheduler"} {
		if !strings.Contains(text, want) {
			t.Fatalf("catalog text missing %q:\n%s", want, text)
		}
	}
}

func TestIgnoreAlerts(t *testing.T) {
	cat, err := Parse([]byte(minimalCatalog))
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	for _, name := range []string{"Watchdog", "watchdog", "  INFOINHIBITOR  "} {
		if !cat.IgnoresAlert(name) {
			t.Fatalf("IgnoresAlert(%q) = false, want true", name)
		}
	}
	for _, name := range []string{"", "RicardoEndpointDown"} {
		if cat.IgnoresAlert(name) {
			t.Fatalf("IgnoresAlert(%q) = true, want false", name)
		}
	}
	text := cat.CatalogText()
	if !strings.Contains(text, "Watchdog") || !strings.Contains(text, "InfoInhibitor") {
		t.Fatalf("catalog text does not mention ignored alerts:\n%s", text)
	}
}

func TestResolveAliases(t *testing.T) {
	cat, err := Parse([]byte(minimalCatalog))
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	cases := map[string]string{
		"планировщик":     "scheduler",
		"ПЛАНИРОВЩИК":     "scheduler",
		"  Планировщик  ": "scheduler",
		"будильники":      "scheduler",
		"scheduler":       "scheduler",
		"Гермес":          "hermes",
		"ГЕРМЁС":          "hermes",
		"гермёс":          "hermes",
		"ассистент":       "hermes",
		"гермес!":         "hermes",
		"хоум ассистент":  "ha",
		"планировщикк":    "scheduler", // one edit away
	}
	for input, wantID := range cases {
		service, err := cat.Resolve(input)
		if err != nil {
			t.Fatalf("Resolve(%q): %v", input, err)
		}
		if service.ID != wantID {
			t.Fatalf("Resolve(%q) = %q, want %q", input, service.ID, wantID)
		}
	}
}

func TestResolveUnknownAndAmbiguous(t *testing.T) {
	cat, err := Parse([]byte(minimalCatalog))
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	for _, input := range []string{"", "   ", "база данных", "x"} {
		if _, err := cat.Resolve(input); err == nil {
			t.Fatalf("Resolve(%q) succeeded, want error", input)
		}
	}
	ambiguous := `
services:
  - id: a
    aliases: {en: [alpha]}
    kind: external
  - id: b
    aliases: {en: [alphabet]}
    kind: external
`
	cat, err = Parse([]byte(ambiguous))
	if err != nil {
		t.Fatalf("Parse ambiguous: %v", err)
	}
	// "alpha" is an exact alias, so it must still win over the substring match.
	if service, err := cat.Resolve("alpha"); err != nil || service.ID != "a" {
		t.Fatalf("exact alias resolve = %q, %v", service.ID, err)
	}
}

func TestValidateErrors(t *testing.T) {
	tests := map[string]string{
		"no services": `
overview: x
`,
		"duplicate id": `
services:
  - {id: a, aliases: {en: [a]}, kind: external}
  - {id: a, aliases: {en: [b]}, kind: external}
`,
		"missing alias": `
services:
  - {id: a, kind: external}
`,
		"bad kind": `
services:
  - {id: a, aliases: {en: [a]}, kind: alien}
`,
		"k8s without namespace": `
services:
  - id: a
    aliases: {en: [a]}
    kind: k8s
    workload: {kind: Deployment, name: a}
`,
		"k8s without workload": `
services:
  - id: a
    aliases: {en: [a]}
    kind: k8s
    namespace: home
`,
		"external with workload": `
services:
  - id: a
    aliases: {en: [a]}
    kind: external
    namespace: home
    workload: {kind: Deployment, name: a}
`,
		"external restart": `
services:
  - id: a
    aliases: {en: [a]}
    kind: external
    restart: {allowed: true, warning: x}
`,
		"restart without warning": `
services:
  - id: a
    aliases: {en: [a]}
    kind: k8s
    namespace: home
    workload: {kind: Deployment, name: a}
    restart: {allowed: true}
`,
		"loki without selector": `
services:
  - id: a
    aliases: {en: [a]}
    kind: k8s
    namespace: home
    workload: {kind: Deployment, name: a}
    logs: {source: loki}
`,
		"bad log source": `
services:
  - id: a
    aliases: {en: [a]}
    kind: k8s
    namespace: home
    workload: {kind: Deployment, name: a}
    logs: {source: file}
`,
		"signal without meaning": `
services:
  - id: a
    aliases: {en: [a]}
    kind: external
    signals: [{promql: up == 0}]
`,
		"bad workload kind": `
services:
  - id: a
    aliases: {en: [a]}
    kind: k8s
    namespace: home
    workload: {kind: Pod, name: a}
`,
		"empty ignore alert": `
ignore_alerts: [Watchdog, ""]
services:
  - {id: a, aliases: {en: [a]}, kind: external}
`,
		"duplicate ignore alert": `
ignore_alerts: [Watchdog, watchdog]
services:
  - {id: a, aliases: {en: [a]}, kind: external}
`,
		"check without label": `
services:
  - {id: a, aliases: {en: [a]}, kind: external}
    checks: [{type: tcp, host: 192.0.2.1, port: 1}]
`,
		"unknown check type": `
services:
  - {id: a, aliases: {en: [a]}, kind: external}
    checks: [{type: ping, label: x}]
`,
		"tcp without port": `
services:
  - {id: a, aliases: {en: [a]}, kind: external}
    checks: [{type: tcp, label: x, host: 192.0.2.1}]
`,
		"bad http auth": `
services:
  - {id: a, aliases: {en: [a]}, kind: external}
    checks: [{type: http, label: x, url: http://192.0.2.1/, auth: basic}]
`,
		"ha_state without entity": `
services:
  - {id: a, aliases: {en: [a]}, kind: external}
    checks: [{type: ha_state, label: x}]
`,
		"ha_template without template": `
services:
  - {id: a, aliases: {en: [a]}, kind: external}
    checks: [{type: ha_template, label: x}]
`,
	}
	for name, raw := range tests {
		t.Run(name, func(t *testing.T) {
			if _, err := Parse([]byte(raw)); err == nil {
				t.Fatalf("Parse succeeded, want validation error")
			}
		})
	}
}

func TestChecksParse(t *testing.T) {
	raw := `
services:
  - id: ha
    aliases: {en: [ha]}
    kind: external
    checks:
      - {type: tcp, label: api, host: 192.0.2.10, port: 8123}
      - {type: http, label: rest, url: "http://192.0.2.10:8123/api/", auth: ha}
      - {type: ha_state, label: satellite, entity_id: assist_satellite.one}
      - {type: ha_template, label: version, template: "{{ 1 }}", required: true}
      - {type: ha_unavailable_summary, label: unavailable, domains: [media_player]}
`
	cat, err := Parse([]byte(raw))
	if err != nil {
		t.Fatalf("Parse: %v", err)
	}
	checks := cat.Services[0].Checks
	if len(checks) != 5 {
		t.Fatalf("checks = %d", len(checks))
	}
	if checks[0].DisplayLabel() != "api" || checks[1].DisplayLabel() != "rest" {
		t.Fatalf("labels = %q %q", checks[0].DisplayLabel(), checks[1].DisplayLabel())
	}
	if !checks[3].Required || checks[4].Domains[0] != "media_player" {
		t.Fatalf("checks = %+v", checks)
	}
}

func TestParseInvalidYAML(t *testing.T) {
	if _, err := Parse([]byte("services: [")); err == nil {
		t.Fatal("Parse succeeded on invalid YAML")
	}
}

func TestExampleCatalogIsValid(t *testing.T) {
	// `go test` runs in the package directory; the Bazel sandbox runs from the
	// runfiles workspace root.
	path := ""
	for _, candidate := range []string{
		"../../testdata/catalog.example.yaml",
		"solutions/mcp-gateway/testdata/catalog.example.yaml",
	} {
		if _, err := os.Stat(candidate); err == nil {
			path = candidate
			break
		}
	}
	if path == "" {
		t.Fatal("catalog.example.yaml not found in runfiles")
	}
	cat, err := LoadFile(path)
	if err != nil {
		t.Fatalf("LoadFile: %v", err)
	}
	if len(cat.Services) == 0 {
		t.Fatal("example catalog has no services")
	}
	for _, service := range cat.Services {
		if _, err := cat.Resolve(service.ID); err != nil {
			t.Fatalf("example service %q does not resolve by id: %v", service.ID, err)
		}
	}
}
