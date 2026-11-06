package store

import (
	"encoding/json"
	"flag"
	"os"
	"testing"

	ricardov1 "github.com/proflyder/monorepo/packages/proto/ricardo/v1"
	"github.com/proflyder/monorepo/solutions/scheduler/internal/idgen"
)

// updateVectors regenerates testdata/ids.json from the Go implementation. The
// file freezes the UUIDv5 event identities and SHA-1 idempotency hashes that a
// future implementation must reproduce byte for byte.
var updateVectors = flag.Bool("update", false, "rewrite testdata/ids.json")

type eventIDVector struct {
	Name      string `json:"name"`
	DedupeKey string `json:"dedupeKey"`
	UUID      string `json:"uuid"`
}

type hashVector struct {
	Name      string `json:"name"`
	Canonical string `json:"canonical"`
	SHA1      string `json:"sha1"`
}

type idsVectors struct {
	Events []eventIDVector `json:"events"`
	Hashes []hashVector    `json:"hashes"`
}

func buildIDVectors() idsVectors {
	dedupeKeys := []eventIDVector{
		{Name: "alarm occurrence", DedupeKey: "trigger:1:sched-alarm:1767339000:0"},
		{Name: "reminder occurrence", DedupeKey: "trigger:2:sched-reminder:1767339000:0"},
		{Name: "timer literal kind", DedupeKey: "trigger:timer:timer-1:1767269100:0"},
		{Name: "scenario occurrence", DedupeKey: "trigger:4:sched-scenario:1767339000:0"},
		{Name: "control stop schedule", DedupeKey: "control:stop:sched-alarm:1767339000:0"},
		{Name: "control stop schedule snooze", DedupeKey: "control:stop:sched-alarm:1767339000:1"},
		{Name: "snoozed fire", DedupeKey: "trigger:1:sched-alarm:1767339000:1"},
		{Name: "expiry stop", DedupeKey: "expiry:stop:1:sched-alarm:1767339000"},
		{Name: "timer cancel stop", DedupeKey: "control:stop:timer:timer-1:1767269100"},
		{Name: "task event", DedupeKey: "task:019b796d-d600-77c5-89f1-c06bc947cfc7:0"},
	}
	for i := range dedupeKeys {
		dedupeKeys[i].UUID = idgen.EventID(dedupeKeys[i].DedupeKey)
	}
	schedules := []struct {
		name string
		in   *ricardov1.Schedule
	}{
		{
			name: "canonical daily alarm",
			in: &ricardov1.Schedule{
				Label: "wake", Kind: ricardov1.ScheduleKind_SCHEDULE_KIND_ALARM,
				Timezone: "UTC", At: &ricardov1.WallTime{Hour: 7, Minute: 30},
				Repeat: ricardov1.RepeatKind_REPEAT_KIND_DAILY, Enabled: true,
				PayloadJson: "{}",
			},
		},
		{
			name: "canonical custom weekdays sorted",
			in: &ricardov1.Schedule{
				Label: "gym", Kind: ricardov1.ScheduleKind_SCHEDULE_KIND_ALARM,
				Timezone: "Asia/Almaty", At: &ricardov1.WallTime{Hour: 6, Minute: 15, Second: 5},
				Repeat: ricardov1.RepeatKind_REPEAT_KIND_CUSTOM, Weekdays: []int32{5, 1, 3},
				Enabled: false, PayloadJson: `{"source":"test"}`,
			},
		},
		{
			name: "canonical once reminder",
			in: &ricardov1.Schedule{
				Label: "call mom", Kind: ricardov1.ScheduleKind_SCHEDULE_KIND_REMINDER,
				Timezone: "Europe/Berlin", At: &ricardov1.WallTime{Hour: 9, Minute: 0},
				OnceDate: "2026-02-01", Repeat: ricardov1.RepeatKind_REPEAT_KIND_ONCE,
				Enabled: true, PayloadJson: "{}",
			},
		},
	}
	hashes := make([]hashVector, 0, len(schedules)+2)
	for _, item := range schedules {
		canonical := canonicalSchedule(item.in)
		hashes = append(hashes, hashVector{Name: item.name, Canonical: canonical, SHA1: idgen.HashRequest(canonical)})
	}
	timerCanonical := canonicalTimer("tea", 300)
	hashes = append(hashes, hashVector{Name: "canonical timer", Canonical: timerCanonical, SHA1: idgen.HashRequest(timerCanonical)})
	taskCanonical := canonicalTask("notify.push", QueueTasks, `{"text":"hi"}`, 1767268800)
	hashes = append(hashes, hashVector{Name: "canonical task", Canonical: taskCanonical, SHA1: idgen.HashRequest(taskCanonical)})
	return idsVectors{Events: dedupeKeys, Hashes: hashes}
}

func idsVectorPath(t *testing.T) string {
	t.Helper()
	candidates := []string{"../../testdata/ids.json", "solutions/scheduler/testdata/ids.json"}
	for _, path := range candidates {
		if _, err := os.Stat(path); err == nil {
			return path
		}
	}
	return candidates[0]
}

func TestIDVectors(t *testing.T) {
	path := idsVectorPath(t)
	want := buildIDVectors()
	if *updateVectors {
		raw, err := json.MarshalIndent(want, "", "  ")
		if err != nil {
			t.Fatalf("encode: %v", err)
		}
		if err := os.WriteFile(path, append(raw, '\n'), 0o644); err != nil {
			t.Fatalf("write: %v", err)
		}
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s: %v", path, err)
	}
	var got idsVectors
	if err := json.Unmarshal(raw, &got); err != nil {
		t.Fatalf("decode %s: %v", path, err)
	}
	if len(got.Events) != len(want.Events) || len(got.Hashes) != len(want.Hashes) {
		t.Fatalf("vector counts = %d/%d, want %d/%d", len(got.Events), len(got.Hashes), len(want.Events), len(want.Hashes))
	}
	for i, item := range got.Events {
		if computed := idgen.EventID(item.DedupeKey); computed != item.UUID {
			t.Fatalf("event %s (%s) = %s, want %s", item.Name, item.DedupeKey, computed, item.UUID)
		}
		if item.UUID != want.Events[i].UUID {
			t.Fatalf("event %s fixture uuid drifted from Go", item.Name)
		}
	}
	for i, item := range got.Hashes {
		if computed := idgen.HashRequest(item.Canonical); computed != item.SHA1 {
			t.Fatalf("hash %s = %s, want %s", item.Name, computed, item.SHA1)
		}
		if item.SHA1 != want.Hashes[i].SHA1 {
			t.Fatalf("hash %s fixture drifted from Go", item.Name)
		}
	}
}
