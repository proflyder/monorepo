package queue

import (
	"testing"

	"github.com/proflyder/monorepo/packages/go/scheduler"
)

func TestMissingCreatedTimeIsUnknownNotEpoch(t *testing.T) {
	created, known := eventCreatedUnix(map[string]any{}, scheduler.Event{})
	if known || created != 0 {
		t.Fatalf("absent created time = (%d, %v), want unknown", created, known)
	}
	created, known = eventCreatedUnix(map[string]any{}, scheduler.Event{CreatedUnix: 1700000000})
	if !known || created != 1700000000 {
		t.Fatalf("event created time = (%d, %v)", created, known)
	}
}
