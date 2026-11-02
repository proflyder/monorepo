package pending

import (
	"testing"
	"time"
)

func TestKeyedIsolatesKeys(t *testing.T) {
	now := time.Date(2026, 11, 3, 10, 0, 0, 0, time.UTC)
	store := NewKeyed[string](time.Minute, func() time.Time { return now })

	store.Put("infra", "restart")
	store.Put("bb", "spawn")

	if got, ok := store.Peek("infra"); !ok || got != "restart" {
		t.Fatalf("infra = %q, %v", got, ok)
	}
	if got, ok := store.Take("bb"); !ok || got != "spawn" {
		t.Fatalf("bb = %q, %v", got, ok)
	}
	if _, ok := store.Peek("bb"); ok {
		t.Fatal("bb survived Take")
	}
	if got, ok := store.Peek("infra"); !ok || got != "restart" {
		t.Fatalf("infra disappeared after taking bb: %q, %v", got, ok)
	}
}

func TestKeyedTTLIsPerEntry(t *testing.T) {
	now := time.Date(2026, 11, 3, 10, 0, 0, 0, time.UTC)
	store := NewKeyed[string](90*time.Second, func() time.Time { return now })
	store.Put("first", "a")
	now = now.Add(60 * time.Second)
	store.Put("second", "b")

	now = now.Add(31 * time.Second) // first is 91s old, second 31s
	if _, ok := store.Peek("first"); ok {
		t.Fatal("first survived its TTL")
	}
	if got, ok := store.Peek("second"); !ok || got != "b" {
		t.Fatalf("second = %q, %v", got, ok)
	}
}

func TestKeyedClear(t *testing.T) {
	store := NewKeyed[int](time.Minute, nil)
	store.Put("k", 1)
	store.Clear("k")
	if _, ok := store.Peek("k"); ok {
		t.Fatal("Clear kept the entry")
	}
}
