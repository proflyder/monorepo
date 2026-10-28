// Package ledger is a small durable receipt ledger for events whose Home
// Assistant side effect already succeeded.
//
// It narrows the duplicate window in at-least-once delivery: a re-delivered
// event that is present in the ledger is acknowledged without calling Home
// Assistant again. It cannot make delivery exactly-once, because a crash
// between the Home Assistant side effect and the ledger write (or between the
// ledger write and the scheduler ack) still produces a duplicate on retry.
package ledger

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"sync"
	"time"
)

const (
	// DefaultRetention bounds how long receipts are kept for deduplication.
	DefaultRetention = 30 * 24 * time.Hour
	// maxReceipts bounds the ledger even without pruning.
	maxReceipts = 20000
	fileVersion = 1
)

// Receipt records one successfully executed event. It never contains tokens or
// payloads, only identifiers needed for deduplication.
type Receipt struct {
	EventID       string `json:"eventId"`
	SourceID      string `json:"sourceId"`
	Action        string `json:"action"`
	Script        string `json:"script"`
	CompletedUnix int64  `json:"completedUnix"`
}

// ActiveAlarm is the alarm occurrence whose sound this bridge started and may
// therefore stop. Only one alarm can be tracked at a time: HA has a single
// global alarm_stop, so a stale STOP must never silence a newer alarm.
//
// OccurrenceUnix and FireDueUnix are both taken from the scheduler event, not
// from bridge wall-clock time. They let a late STOP for an older occurrence or
// an earlier due time be refused even when the source id still matches (for
// example after a snooze re-fire).
type ActiveAlarm struct {
	SourceID       string `json:"sourceId"`
	EventID        string `json:"eventId"`
	OccurrenceUnix int64  `json:"occurrenceUnix,omitempty"`
	FireDueUnix    int64  `json:"fireDueUnix,omitempty"`
	SinceUnix      int64  `json:"sinceUnix"`
}

type fileFormat struct {
	Version     int                `json:"version"`
	Receipts    map[string]Receipt `json:"receipts"`
	ActiveAlarm *ActiveAlarm       `json:"activeAlarm,omitempty"`
}

// Ledger is safe for concurrent use.
type Ledger struct {
	path     string
	mu       sync.Mutex
	receipts map[string]Receipt
	active   *ActiveAlarm
	now      func() time.Time
}

// Open loads a file-backed ledger, creating it on first write. An empty path
// returns an in-memory ledger that still deduplicates within the process run.
// A corrupt existing file is an error: silently starting empty could duplicate
// a Home Assistant side effect.
func Open(path string) (*Ledger, error) {
	l := &Ledger{path: path, receipts: make(map[string]Receipt), now: time.Now}
	if path == "" {
		return l, nil
	}
	data, err := os.ReadFile(path)
	if errors.Is(err, os.ErrNotExist) {
		return l, nil
	}
	if err != nil {
		return nil, fmt.Errorf("ledger: read %s: %w", path, err)
	}
	if len(data) == 0 {
		return l, nil
	}
	var stored fileFormat
	if err := json.Unmarshal(data, &stored); err != nil {
		return nil, fmt.Errorf("ledger: %s is corrupt: %w", path, err)
	}
	if stored.Version != fileVersion {
		return nil, fmt.Errorf("ledger: %s has unsupported version %d", path, stored.Version)
	}
	for id, receipt := range stored.Receipts {
		if receipt.EventID == "" {
			receipt.EventID = id
		}
		l.receipts[id] = receipt
	}
	if stored.ActiveAlarm != nil && stored.ActiveAlarm.SourceID != "" {
		active := *stored.ActiveAlarm
		l.active = &active
	}
	if _, err := l.pruneLocked(); err != nil {
		return nil, err
	}
	return l, nil
}

// NewMemory returns an in-memory ledger.
func NewMemory() *Ledger {
	return &Ledger{receipts: make(map[string]Receipt), now: time.Now}
}

// Path reports the backing file, or "" for an in-memory ledger.
func (l *Ledger) Path() string { return l.path }

// Seen reports whether the event side effect was already recorded.
func (l *Ledger) Seen(eventID string) bool {
	if l == nil || eventID == "" {
		return false
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	_, ok := l.receipts[eventID]
	return ok
}

// Record stores a receipt and persists it with an atomic rename. The in-memory
// entry is kept even if persistence fails so the current process still
// deduplicates; the caller must log the error because a restart would retry.
func (l *Ledger) Record(receipt Receipt) error {
	if l == nil {
		return errors.New("ledger: nil")
	}
	if err := receipt.validate(); err != nil {
		return err
	}
	receipt = l.stamp(receipt)
	l.mu.Lock()
	defer l.mu.Unlock()
	l.receipts[receipt.EventID] = receipt
	l.trimLocked()
	if l.path == "" {
		return nil
	}
	return l.persistLocked()
}

func (r Receipt) validate() error {
	if r.EventID == "" {
		return errors.New("ledger: empty event id")
	}
	return nil
}

func (l *Ledger) stamp(receipt Receipt) Receipt {
	if receipt.CompletedUnix == 0 {
		receipt.CompletedUnix = l.now().Unix()
	}
	return receipt
}

// ActiveAlarm reports the alarm source marked as the active sound.
func (l *Ledger) ActiveAlarm() (ActiveAlarm, bool) {
	if l == nil {
		return ActiveAlarm{}, false
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	if l.active == nil || l.active.SourceID == "" {
		return ActiveAlarm{}, false
	}
	return *l.active, true
}

// RecordStart atomically stores the receipt of an accepted alarm FIRE and marks
// its source as the active alarm. The marker is restored after a restart from
// the same JSON file.
func (l *Ledger) RecordStart(receipt Receipt, active ActiveAlarm) error {
	if l == nil {
		return errors.New("ledger: nil")
	}
	if err := receipt.validate(); err != nil {
		return err
	}
	if active.SourceID == "" {
		return errors.New("ledger: empty active alarm source id")
	}
	receipt = l.stamp(receipt)
	marker := active
	if marker.SinceUnix == 0 {
		marker.SinceUnix = l.now().Unix()
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	l.receipts[receipt.EventID] = receipt
	l.active = &marker
	l.trimLocked()
	if l.path == "" {
		return nil
	}
	return l.persistLocked()
}

// RecordStop stores the receipt of an accepted alarm STOP and clears the active
// marker only when it still refers to expectSourceID. It returns whether the
// marker was cleared; a concurrent newer alarm keeps its marker.
func (l *Ledger) RecordStop(receipt Receipt, expectSourceID string) (bool, error) {
	if l == nil {
		return false, errors.New("ledger: nil")
	}
	if err := receipt.validate(); err != nil {
		return false, err
	}
	receipt = l.stamp(receipt)
	l.mu.Lock()
	defer l.mu.Unlock()
	l.receipts[receipt.EventID] = receipt
	cleared := l.active != nil && l.active.SourceID == expectSourceID
	if cleared {
		l.active = nil
	}
	l.trimLocked()
	if l.path == "" {
		return cleared, nil
	}
	return cleared, l.persistLocked()
}

// Prune removes receipts older than the retention window.
func (l *Ledger) Prune() error {
	if l == nil || l.path == "" {
		return nil
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	removed, err := l.pruneLocked()
	if err != nil || removed == 0 {
		return err
	}
	return l.persistLocked()
}

// Len reports the number of stored receipts.
func (l *Ledger) Len() int {
	if l == nil {
		return 0
	}
	l.mu.Lock()
	defer l.mu.Unlock()
	return len(l.receipts)
}

func (l *Ledger) pruneLocked() (int, error) {
	cutoff := l.now().Add(-DefaultRetention).Unix()
	removed := 0
	for id, receipt := range l.receipts {
		if receipt.CompletedUnix < cutoff {
			delete(l.receipts, id)
			removed++
		}
	}
	if removed == 0 {
		return 0, nil
	}
	return removed, l.persistLocked()
}

func (l *Ledger) trimLocked() {
	if len(l.receipts) <= maxReceipts {
		return
	}
	type pair struct {
		id       string
		complete int64
	}
	all := make([]pair, 0, len(l.receipts))
	for id, receipt := range l.receipts {
		all = append(all, pair{id: id, complete: receipt.CompletedUnix})
	}
	sort.Slice(all, func(i, j int) bool { return all[i].complete < all[j].complete })
	for _, item := range all[:len(all)-maxReceipts] {
		delete(l.receipts, item.id)
	}
}

func (l *Ledger) persistLocked() error {
	dir := filepath.Dir(l.path)
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return fmt.Errorf("ledger: create directory: %w", err)
	}
	data, err := json.Marshal(fileFormat{
		Version:     fileVersion,
		Receipts:    l.receipts,
		ActiveAlarm: l.active,
	})
	if err != nil {
		return fmt.Errorf("ledger: encode: %w", err)
	}

	tmp, err := os.CreateTemp(dir, ".ha-bridge-ledger-*.tmp")
	if err != nil {
		return fmt.Errorf("ledger: create temp file: %w", err)
	}
	tmpName := tmp.Name()
	cleanup := func() {
		_ = tmp.Close()
		_ = os.Remove(tmpName)
	}
	if _, err := tmp.Write(data); err != nil {
		cleanup()
		return fmt.Errorf("ledger: write temp file: %w", err)
	}
	if err := tmp.Sync(); err != nil {
		cleanup()
		return fmt.Errorf("ledger: sync temp file: %w", err)
	}
	if err := tmp.Close(); err != nil {
		_ = os.Remove(tmpName)
		return fmt.Errorf("ledger: close temp file: %w", err)
	}
	if err := os.Rename(tmpName, l.path); err != nil {
		_ = os.Remove(tmpName)
		return fmt.Errorf("ledger: replace %s: %w", l.path, err)
	}
	if dirHandle, err := os.Open(dir); err == nil {
		_ = dirHandle.Sync()
		_ = dirHandle.Close()
	}
	return nil
}
