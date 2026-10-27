package app

import "testing"

func TestName(t *testing.T) {
	if Name != "alarm" {
		t.Fatalf("unexpected service name: %q", Name)
	}
}
