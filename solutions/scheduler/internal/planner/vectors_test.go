package planner

import (
	"encoding/json"
	"flag"
	"os"
	"path/filepath"
	"testing"
	"time"

	ricardov1 "github.com/proflyder/monorepo/packages/proto/ricardo/v1"
)

// updateVectors regenerates testdata/planner/*.json from the Go planner. The
// files are the golden table that the Rust implementation must reproduce.
var updateVectors = flag.Bool("update", false, "rewrite testdata planner vectors")

type plannerRule struct {
	Timezone string  `json:"timezone"`
	Hour     int     `json:"hour"`
	Minute   int     `json:"minute"`
	Second   int     `json:"second"`
	Repeat   string  `json:"repeat"`
	Weekdays []int32 `json:"weekdays,omitempty"`
	OnceDate string  `json:"onceDate,omitempty"`
}

type plannerVector struct {
	Name         string      `json:"name"`
	Rule         plannerRule `json:"rule"`
	AfterUnix    int64       `json:"afterUnix"`
	ExpectedUnix *int64      `json:"expectedUnix"`
}

var repeatKinds = map[string]ricardov1.RepeatKind{
	"REPEAT_KIND_ONCE":     ricardov1.RepeatKind_REPEAT_KIND_ONCE,
	"REPEAT_KIND_DAILY":    ricardov1.RepeatKind_REPEAT_KIND_DAILY,
	"REPEAT_KIND_WEEKDAYS": ricardov1.RepeatKind_REPEAT_KIND_WEEKDAYS,
	"REPEAT_KIND_WEEKENDS": ricardov1.RepeatKind_REPEAT_KIND_WEEKENDS,
	"REPEAT_KIND_CUSTOM":   ricardov1.RepeatKind_REPEAT_KIND_CUSTOM,
}

func repeatName(kind ricardov1.RepeatKind) string {
	for name, value := range repeatKinds {
		if value == kind {
			return name
		}
	}
	return "REPEAT_KIND_UNSPECIFIED"
}

func vectorTime(value string) time.Time {
	t, err := time.Parse(time.RFC3339, value)
	if err != nil {
		panic(err)
	}
	return t.UTC()
}

func vectorCases() map[string][]plannerVector {
	build := func(name string, rule Rule, after time.Time) plannerVector {
		next, ok, err := rule.Next(after)
		if err != nil {
			panic(err)
		}
		item := plannerVector{
			Name:      name,
			AfterUnix: after.Unix(),
			Rule: plannerRule{
				Timezone: rule.Timezone,
				Hour:     rule.Hour,
				Minute:   rule.Minute,
				Second:   rule.Second,
				Repeat:   repeatName(rule.Repeat),
				Weekdays: rule.Weekdays,
				OnceDate: rule.OnceDate,
			},
		}
		if ok {
			unix := next.Unix()
			item.ExpectedUnix = &unix
		}
		return item
	}
	return map[string][]plannerVector{
		"utc": {
			build("daily", Rule{Timezone: "UTC", Hour: 7, Minute: 30, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}, vectorTime("2026-01-01T12:00:00Z")),
			build("once", Rule{Timezone: "UTC", Hour: 9, Minute: 0, Repeat: ricardov1.RepeatKind_REPEAT_KIND_ONCE, OnceDate: "2026-01-05"}, vectorTime("2026-01-01T12:00:00Z")),
			build("once exhausted", Rule{Timezone: "UTC", Hour: 9, Minute: 0, Repeat: ricardov1.RepeatKind_REPEAT_KIND_ONCE, OnceDate: "2025-12-31"}, vectorTime("2026-01-01T12:00:00Z")),
			build("weekdays from friday", Rule{Timezone: "UTC", Hour: 6, Minute: 15, Repeat: ricardov1.RepeatKind_REPEAT_KIND_WEEKDAYS}, vectorTime("2026-01-02T12:00:00Z")),
			build("weekends from friday", Rule{Timezone: "UTC", Hour: 8, Minute: 0, Repeat: ricardov1.RepeatKind_REPEAT_KIND_WEEKENDS}, vectorTime("2026-01-02T12:00:00Z")),
			build("custom mon wed fri", Rule{Timezone: "UTC", Hour: 6, Minute: 15, Repeat: ricardov1.RepeatKind_REPEAT_KIND_CUSTOM, Weekdays: []int32{1, 3, 5}}, vectorTime("2026-01-02T12:00:00Z")),
			build("last second", Rule{Timezone: "UTC", Hour: 23, Minute: 59, Second: 59, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}, vectorTime("2026-01-01T23:59:59Z")),
		},
		"almaty": {
			build("daily", Rule{Timezone: "Asia/Almaty", Hour: 7, Minute: 30, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}, vectorTime("2026-01-01T12:00:00Z")),
			build("evening", Rule{Timezone: "Asia/Almaty", Hour: 21, Minute: 0, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}, vectorTime("2026-06-01T00:00:00Z")),
		},
		"berlin": {
			build("spring gap skipped", Rule{Timezone: "Europe/Berlin", Hour: 2, Minute: 30, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}, vectorTime("2026-03-28T12:00:00Z")),
			build("fall fold earliest", Rule{Timezone: "Europe/Berlin", Hour: 2, Minute: 30, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}, vectorTime("2026-10-24T12:00:00Z")),
			build("summer daily", Rule{Timezone: "Europe/Berlin", Hour: 7, Minute: 0, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}, vectorTime("2026-07-01T00:00:00Z")),
		},
		"new_york": {
			build("fall fold earliest", Rule{Timezone: "America/New_York", Hour: 1, Minute: 30, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}, vectorTime("2026-10-31T12:00:00Z")),
			build("spring gap skipped", Rule{Timezone: "America/New_York", Hour: 2, Minute: 30, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}, vectorTime("2026-03-07T12:00:00Z")),
		},
		"apia": {
			build("skipped civil date", Rule{Timezone: "Pacific/Apia", Hour: 8, Minute: 0, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}, vectorTime("2011-12-29T22:00:00Z")),
			build("after skipped date", Rule{Timezone: "Pacific/Apia", Hour: 8, Minute: 0, Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY}, vectorTime("2011-12-30T12:00:00Z")),
		},
	}
}

func plannerVectorDir(t *testing.T) string {
	t.Helper()
	candidates := []string{"../../testdata/planner", "solutions/scheduler/testdata/planner"}
	for _, dir := range candidates {
		if info, err := os.Stat(dir); err == nil && info.IsDir() {
			return dir
		}
	}
	return candidates[0]
}

func TestPlannerVectors(t *testing.T) {
	dir := plannerVectorDir(t)
	cases := vectorCases()
	if *updateVectors {
		if err := os.MkdirAll(dir, 0o755); err != nil {
			t.Fatalf("mkdir: %v", err)
		}
		for name, vectors := range cases {
			raw, err := json.MarshalIndent(vectors, "", "  ")
			if err != nil {
				t.Fatalf("encode %s: %v", name, err)
			}
			if err := os.WriteFile(filepath.Join(dir, name+".json"), append(raw, '\n'), 0o644); err != nil {
				t.Fatalf("write %s: %v", name, err)
			}
		}
	}
	files, err := filepath.Glob(filepath.Join(dir, "*.json"))
	if err != nil || len(files) == 0 {
		t.Fatalf("no planner vectors in %s: %v", dir, err)
	}
	for _, path := range files {
		raw, err := os.ReadFile(path)
		if err != nil {
			t.Fatalf("read %s: %v", path, err)
		}
		var vectors []plannerVector
		if err := json.Unmarshal(raw, &vectors); err != nil {
			t.Fatalf("decode %s: %v", path, err)
		}
		for _, item := range vectors {
			item := item
			t.Run(filepath.Base(path)+"/"+item.Name, func(t *testing.T) {
				kind, ok := repeatKinds[item.Rule.Repeat]
				if !ok {
					t.Fatalf("unknown repeat %q", item.Rule.Repeat)
				}
				rule := Rule{
					Timezone: item.Rule.Timezone,
					Hour:     item.Rule.Hour,
					Minute:   item.Rule.Minute,
					Second:   item.Rule.Second,
					Repeat:   kind,
					Weekdays: item.Rule.Weekdays,
					OnceDate: item.Rule.OnceDate,
				}
				next, ok, err := rule.Next(time.Unix(item.AfterUnix, 0))
				if err != nil {
					t.Fatalf("next: %v", err)
				}
				if item.ExpectedUnix == nil {
					if ok {
						t.Fatalf("got occurrence %d, want exhausted", next.Unix())
					}
					return
				}
				if !ok {
					t.Fatalf("rule exhausted, want %d", *item.ExpectedUnix)
				}
				if next.Unix() != *item.ExpectedUnix {
					t.Fatalf("next = %d, want %d", next.Unix(), *item.ExpectedUnix)
				}
			})
		}
	}
}
