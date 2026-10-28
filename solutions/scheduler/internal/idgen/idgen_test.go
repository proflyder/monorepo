package idgen

import (
	"strings"
	"testing"
	"time"
)

func TestEventIDStableAndDistinct(t *testing.T) {
	a := EventID("trigger:1:abc:1700000000:0")
	b := EventID("trigger:1:abc:1700000000:0")
	c := EventID("trigger:1:abc:1700000001:0")
	if a != b {
		t.Fatalf("same dedupe key produced different ids: %s != %s", a, b)
	}
	if a == c {
		t.Fatalf("different occurrences produced the same id: %s", a)
	}
	if len(a) != 36 || strings.Count(a, "-") != 4 {
		t.Fatalf("id is not UUID formatted: %s", a)
	}
	if a[14] != '5' {
		t.Fatalf("event id is not UUIDv5: %s", a)
	}
}

func TestResourceIDsAreOrderedAndUnique(t *testing.T) {
	base := time.Date(2026, 1, 1, 0, 0, 0, 0, time.UTC)
	first, err := NewResourceID(base)
	if err != nil {
		t.Fatalf("NewResourceID: %v", err)
	}
	second, err := NewResourceID(base.Add(time.Millisecond))
	if err != nil {
		t.Fatalf("NewResourceID: %v", err)
	}
	if first == second {
		t.Fatalf("resource ids collided: %s", first)
	}
	if first[14] != '7' {
		t.Fatalf("resource id is not UUIDv7: %s", first)
	}
	if !(first < second) {
		t.Fatalf("UUIDv7 not time ordered: %s >= %s", first, second)
	}
}
