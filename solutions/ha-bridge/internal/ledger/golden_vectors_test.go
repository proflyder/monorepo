package ledger

import (
	"encoding/json"
	"flag"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"testing"
	"time"
)

// updateGolden rewrites testdata/ledger/v1.json from the Go ledger. The file is
// the shared golden table for the Rust ledger: the same bytes on disk, the same
// retention window and the same 20 000 receipt cap.
var updateGolden = flag.Bool("update", false, "rewrite the ha-bridge ledger golden vector")

const (
	goldenClock     = int64(1767225600) // 2026-01-01T00:00:00Z
	goldenRetention = int64(30 * 24 * 60 * 60)
)

type ledgerFixture struct {
	ClockUnix   int64         `json:"clockUnix"`
	Canonical   string        `json:"canonical"`
	Replay      []Receipt     `json:"replay"`
	ActiveAlarm *ActiveAlarm  `json:"activeAlarm,omitempty"`
	Prune       pruneFixture  `json:"prune"`
	Trim        trimFixture   `json:"trim"`
	Invalid     []invalidCase `json:"invalid"`
}

type pruneFixture struct {
	RetentionSeconds int64     `json:"retentionSeconds"`
	Receipts         []Receipt `json:"receipts"`
	ExpectedKept     []string  `json:"expectedKept"`
}

type trimFixture struct {
	MaxReceipts        int   `json:"maxReceipts"`
	Count              int   `json:"count"`
	FirstCompletedUnix int64 `json:"firstCompletedUnix"`
	Step               int64 `json:"step"`
	ExpectedKept       int   `json:"expectedKept"`
	ExpectedRemoved    int   `json:"expectedRemoved"`
}

type invalidCase struct {
	Name  string `json:"name"`
	Body  string `json:"body"`
	Error string `json:"error"`
}

func replayReceipts() ([]Receipt, *ActiveAlarm) {
	receipts := []Receipt{
		{
			EventID: "evt-11111111-1111-5111-8111-111111111111", SourceID: "alarm-1",
			Action: "FIRE", Script: "alarm_start", CompletedUnix: goldenClock - 600,
		},
		{
			EventID: "evt-22222222-2222-5222-8222-222222222222", SourceID: "alarm-2",
			Action: "STOP", Script: "alarm_stop", CompletedUnix: goldenClock - 300,
		},
	}
	active := &ActiveAlarm{
		SourceID:       "alarm-1",
		EventID:        receipts[0].EventID,
		OccurrenceUnix: goldenClock - 900,
		FireDueUnix:    goldenClock - 600,
		SinceUnix:      goldenClock - 600,
	}
	return receipts, active
}

func openAt(t *testing.T, path string, clockUnix int64) *Ledger {
	t.Helper()
	return &Ledger{
		path:     path,
		receipts: make(map[string]Receipt),
		now:      func() time.Time { return time.Unix(clockUnix, 0).UTC() },
	}
}

// canonicalBytes records the replay receipts in a fresh file and returns the
// exact bytes an atomic write leaves on disk.
func canonicalBytes(t *testing.T) ([]byte, []Receipt, *ActiveAlarm) {
	t.Helper()
	path := filepath.Join(t.TempDir(), "ledger.json")
	receipts, active := replayReceipts()
	l := openAt(t, path, goldenClock)
	if err := l.RecordStart(receipts[0], *active); err != nil {
		t.Fatalf("RecordStart: %v", err)
	}
	if err := l.Record(receipts[1]); err != nil {
		t.Fatalf("Record: %v", err)
	}
	data, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read canonical ledger: %v", err)
	}
	return data, receipts, active
}

func pruneVector(t *testing.T) pruneFixture {
	t.Helper()
	receipts := []Receipt{
		{EventID: "old", CompletedUnix: goldenClock - goldenRetention - 1},
		{EventID: "edge", CompletedUnix: goldenClock - goldenRetention},
		{EventID: "new", CompletedUnix: goldenClock},
	}
	path := filepath.Join(t.TempDir(), "ledger.json")
	l := openAt(t, path, goldenClock)
	for _, receipt := range receipts {
		if err := l.Record(receipt); err != nil {
			t.Fatalf("Record %s: %v", receipt.EventID, err)
		}
	}
	if _, err := l.pruneLocked(); err != nil {
		t.Fatalf("prune: %v", err)
	}
	kept := make([]string, 0, len(l.receipts))
	for id := range l.receipts {
		kept = append(kept, id)
	}
	sort.Strings(kept)
	return pruneFixture{
		RetentionSeconds: goldenRetention,
		Receipts:         receipts,
		ExpectedKept:     kept,
	}
}

func trimVector(t *testing.T) trimFixture {
	t.Helper()
	count := maxReceipts + 5
	first := int64(1767000000)
	l := openAt(t, "", goldenClock)
	for i := range count {
		id := "evt-" + strconv.Itoa(i)
		l.receipts[id] = Receipt{EventID: id, CompletedUnix: first + int64(i)}
	}
	l.trimLocked()
	return trimFixture{
		MaxReceipts:        maxReceipts,
		Count:              count,
		FirstCompletedUnix: first,
		Step:               1,
		ExpectedKept:       len(l.receipts),
		ExpectedRemoved:    count - len(l.receipts),
	}
}

func invalidVectors() []invalidCase {
	unsupported, err := json.Marshal(fileFormat{Version: 99, Receipts: map[string]Receipt{}})
	if err != nil {
		panic(err)
	}
	return []invalidCase{
		{Name: "corrupt", Body: "{not json", Error: "is corrupt"},
		{Name: "unsupported version", Body: string(unsupported), Error: "unsupported version 99"},
	}
}

func buildFixture(t *testing.T) ledgerFixture {
	t.Helper()
	canonical, receipts, active := canonicalBytes(t)
	return ledgerFixture{
		ClockUnix:   goldenClock,
		Canonical:   string(canonical),
		Replay:      receipts,
		ActiveAlarm: active,
		Prune:       pruneVector(t),
		Trim:        trimVector(t),
		Invalid:     invalidVectors(),
	}
}

func goldenPath(t *testing.T) string {
	t.Helper()
	if root := os.Getenv("TEST_SRCDIR"); root != "" {
		workspace := os.Getenv("TEST_WORKSPACE")
		for _, path := range []string{
			filepath.Join(root, workspace, "solutions/ha-bridge/testdata/ledger/v1.json"),
			filepath.Join(root, "solutions/ha-bridge/testdata/ledger/v1.json"),
		} {
			if _, err := os.Stat(path); err == nil {
				return path
			}
		}
	}
	candidates := []string{
		"../../testdata/ledger/v1.json",
		"solutions/ha-bridge/testdata/ledger/v1.json",
	}
	for _, path := range candidates {
		if _, err := os.Stat(filepath.Dir(path)); err == nil {
			return path
		}
	}
	return candidates[0]
}

func TestGoldenLedgerVector(t *testing.T) {
	fixture := buildFixture(t)
	data, err := json.MarshalIndent(fixture, "", "  ")
	if err != nil {
		t.Fatalf("encode: %v", err)
	}
	data = append(data, '\n')
	path := goldenPath(t)
	if *updateGolden {
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatalf("create directory: %v", err)
		}
		if err := os.WriteFile(path, data, 0o644); err != nil {
			t.Fatalf("write %s: %v", path, err)
		}
		t.Logf("wrote %s", path)
		return
	}
	got, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read %s (run go test -run TestGoldenLedgerVector -update): %v", path, err)
	}
	if string(got) != string(data) {
		t.Errorf("%s does not match the Go ledger; run go test -run TestGoldenLedgerVector -update\nwant:\n%s\ngot:\n%s", path, data, got)
	}
}
