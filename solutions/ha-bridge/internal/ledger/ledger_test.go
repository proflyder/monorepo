package ledger

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strconv"
	"testing"
	"time"
)

func TestRecordAndReopen(t *testing.T) {
	path := filepath.Join(t.TempDir(), "state", "ledger.json")

	first, err := Open(path)
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	if first.Seen("evt-1") {
		t.Fatal("fresh ledger already contains evt-1")
	}
	if err := first.Record(Receipt{EventID: "evt-1", SourceID: "alarm-1", Action: "FIRE", Script: "alarm_start"}); err != nil {
		t.Fatalf("Record: %v", err)
	}
	if !first.Seen("evt-1") {
		t.Fatal("evt-1 not visible after Record")
	}

	second, err := Open(path)
	if err != nil {
		t.Fatalf("reopen: %v", err)
	}
	if !second.Seen("evt-1") {
		t.Fatal("receipt did not survive reopening")
	}
	if second.Len() != 1 {
		t.Errorf("Len = %d, want 1", second.Len())
	}

	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if mode := info.Mode().Perm(); mode != 0o600 {
		t.Errorf("ledger mode = %o, want 600", mode)
	}

	leftovers, err := filepath.Glob(filepath.Join(filepath.Dir(path), ".ha-bridge-ledger-*"))
	if err != nil {
		t.Fatal(err)
	}
	if len(leftovers) != 0 {
		t.Errorf("temporary files left behind: %v", leftovers)
	}
}

func TestRecordRequiresEventID(t *testing.T) {
	l := NewMemory()
	if err := l.Record(Receipt{}); err == nil {
		t.Fatal("empty event id must be rejected")
	}
}

func TestCorruptFileFailsLoudly(t *testing.T) {
	path := filepath.Join(t.TempDir(), "ledger.json")
	if err := os.WriteFile(path, []byte("{not json"), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := Open(path); err == nil {
		t.Fatal("a corrupt ledger must not silently start empty")
	}
}

func TestUnsupportedVersionFails(t *testing.T) {
	path := filepath.Join(t.TempDir(), "ledger.json")
	data, err := json.Marshal(fileFormat{Version: 99, Receipts: map[string]Receipt{}})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := Open(path); err == nil {
		t.Fatal("an unknown ledger version must be rejected")
	}
}

func TestPruneDropsExpiredReceipts(t *testing.T) {
	path := filepath.Join(t.TempDir(), "ledger.json")
	old := time.Now().Add(-DefaultRetention - time.Hour).Unix()
	data, err := json.Marshal(fileFormat{Version: fileVersion, Receipts: map[string]Receipt{
		"old": {EventID: "old", CompletedUnix: old},
		"new": {EventID: "new", CompletedUnix: time.Now().Unix()},
	}})
	if err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, data, 0o600); err != nil {
		t.Fatal(err)
	}

	l, err := Open(path)
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	if l.Seen("old") {
		t.Error("expired receipt survived pruning")
	}
	if !l.Seen("new") {
		t.Error("fresh receipt was pruned")
	}
}

func TestTrimKeepsLedgerBounded(t *testing.T) {
	l := NewMemory()
	for i := 0; i < maxReceipts+10; i++ {
		if err := l.Record(Receipt{EventID: "evt-" + strconv.Itoa(i), CompletedUnix: int64(i)}); err != nil {
			t.Fatal(err)
		}
	}
	if l.Len() > maxReceipts {
		t.Errorf("Len = %d, want <= %d", l.Len(), maxReceipts)
	}
}

func TestNilAndMemoryLedgers(t *testing.T) {
	var nilLedger *Ledger
	if nilLedger.Seen("evt") {
		t.Error("nil ledger must not report seen events")
	}

	memory := NewMemory()
	if memory.Path() != "" {
		t.Errorf("memory ledger path = %q", memory.Path())
	}
	if err := memory.Record(Receipt{EventID: "evt-1"}); err != nil {
		t.Fatal(err)
	}
	if !memory.Seen("evt-1") {
		t.Error("memory ledger did not record the receipt")
	}
	if err := memory.Prune(); err != nil {
		t.Errorf("Prune on memory ledger: %v", err)
	}
}

func TestActiveAlarmPersistsAndClearsOnlyOnMatch(t *testing.T) {
	path := filepath.Join(t.TempDir(), "ledger.json")

	first, err := Open(path)
	if err != nil {
		t.Fatalf("Open: %v", err)
	}
	start := Receipt{EventID: "evt-a-fire", SourceID: "alarm-a", Action: "FIRE", Script: "alarm_start"}
	if err := first.RecordStart(start, ActiveAlarm{
		SourceID:       "alarm-a",
		EventID:        start.EventID,
		OccurrenceUnix: 1700000000,
		FireDueUnix:    1700000060,
	}); err != nil {
		t.Fatalf("RecordStart: %v", err)
	}

	reopened, err := Open(path)
	if err != nil {
		t.Fatalf("reopen: %v", err)
	}
	active, ok := reopened.ActiveAlarm()
	if !ok || active.SourceID != "alarm-a" || active.EventID != start.EventID {
		t.Fatalf("restored active alarm = %+v, %v", active, ok)
	}
	if active.OccurrenceUnix != 1700000000 || active.FireDueUnix != 1700000060 {
		t.Fatalf("restored occurrence/due guard = %+v, want the scheduler values", active)
	}

	// A STOP for a different source must neither clear the marker nor lose it.
	cleared, err := reopened.RecordStop(
		Receipt{EventID: "evt-b-stop", SourceID: "alarm-b", Action: "STOP", Script: "alarm_stop"},
		"alarm-b",
	)
	if err != nil {
		t.Fatalf("RecordStop mismatch: %v", err)
	}
	if cleared {
		t.Fatal("RecordStop cleared an unrelated active alarm")
	}
	if active, _ := reopened.ActiveAlarm(); active.SourceID != "alarm-a" {
		t.Fatalf("active alarm after mismatched STOP = %+v", active)
	}

	// The matching STOP clears the marker and the cleared state survives.
	cleared, err = reopened.RecordStop(
		Receipt{EventID: "evt-a-stop", SourceID: "alarm-a", Action: "STOP", Script: "alarm_stop"},
		"alarm-a",
	)
	if err != nil {
		t.Fatalf("RecordStop: %v", err)
	}
	if !cleared {
		t.Fatal("RecordStop did not clear the matching active alarm")
	}
	final, err := Open(path)
	if err != nil {
		t.Fatalf("final reopen: %v", err)
	}
	if _, ok := final.ActiveAlarm(); ok {
		t.Error("cleared active alarm came back after reopening")
	}
}

func TestRecordStartRequiresSourceID(t *testing.T) {
	ledger := NewMemory()
	err := ledger.RecordStart(Receipt{EventID: "evt-a"}, ActiveAlarm{})
	if err == nil {
		t.Fatal("empty active alarm source id must be rejected")
	}
}
