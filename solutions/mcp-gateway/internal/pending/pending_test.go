package pending

import (
	"testing"
	"time"
)

func TestStoreReplacePeekTake(t *testing.T) {
	now := time.Date(2026, 11, 1, 10, 0, 0, 0, time.UTC)
	store := New(90*time.Second, func() time.Time { return now })

	store.Put(Action{ServiceID: "scheduler"})
	store.Put(Action{ServiceID: "hermes"})

	got, ok := store.Peek()
	if !ok || got.ServiceID != "hermes" {
		t.Fatalf("Peek = %+v, %v; want hermes", got, ok)
	}
	if again, ok := store.Peek(); !ok || again.ServiceID != "hermes" {
		t.Fatalf("Peek is not idempotent: %+v, %v", again, ok)
	}
	got, ok = store.Take()
	if !ok || got.ServiceID != "hermes" {
		t.Fatalf("Take = %+v, %v", got, ok)
	}
	if _, ok := store.Take(); ok {
		t.Fatal("Take after Take returned an action")
	}
}

func TestStoreTTL(t *testing.T) {
	now := time.Date(2026, 11, 1, 10, 0, 0, 0, time.UTC)
	store := New(90*time.Second, func() time.Time { return now })
	store.Put(Action{ServiceID: "scheduler"})

	now = now.Add(89 * time.Second)
	if _, ok := store.Peek(); !ok {
		t.Fatal("action expired before TTL")
	}
	now = now.Add(2 * time.Second)
	if _, ok := store.Peek(); ok {
		t.Fatal("action survived its TTL")
	}
	if _, ok := store.Take(); ok {
		t.Fatal("Take returned an expired action")
	}
}

func TestStoreClear(t *testing.T) {
	store := New(time.Minute, nil)
	store.Put(Action{ServiceID: "scheduler"})
	store.Clear()
	if _, ok := store.Peek(); ok {
		t.Fatal("Clear did not drop the action")
	}
}

func TestStoreCreatedAtDefault(t *testing.T) {
	now := time.Date(2026, 11, 1, 10, 0, 0, 0, time.UTC)
	store := New(time.Minute, func() time.Time { return now })
	store.Put(Action{ServiceID: "scheduler"})
	got, ok := store.Peek()
	if !ok {
		t.Fatal("missing action")
	}
	if !got.CreatedAt.Equal(now) {
		t.Fatalf("CreatedAt = %v, want %v", got.CreatedAt, now)
	}
}
