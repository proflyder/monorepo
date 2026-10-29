// Package catalog is the phone-facing view of controllable Home Assistant
// entities. Nothing here is listed by hand: entries are built from discovered
// Home Assistant data by Build, filtered by rules (see rules.go), and resolved
// back to a service call by Resolve. Only entries that survive the rules can be
// shown or driven, so neither the phone nor a task payload can reach an
// arbitrary Home Assistant service or entity.
package catalog

import (
	"fmt"
	"strings"
)

// Kind is the control model of an entry.
type Kind string

const (
	// KindScenario is a one-shot action: an HA script or scene. Action "run".
	KindScenario Kind = "scenario"
	// KindLight and KindSwitch are stateful devices with on/off.
	KindLight  Kind = "light"
	KindSwitch Kind = "switch"
)

// Actions understood by the executor.
const (
	ActionRun     = "run"
	ActionTurnOn  = "turn_on"
	ActionTurnOff = "turn_off"
)

// TaskType is the scheduler task type of a control command.
const TaskType = "home.control"

// Entry is one controllable thing, with its live state at discovery time.
type Entry struct {
	// ID is the catalog id the phone sends back. It is the Home Assistant
	// entity id; the executor only honours ids that are in the current catalog.
	ID    string
	Kind  Kind
	Title string
	// Room is the HA area name ("" when unassigned).
	Room string
	// Icon is a hint from HA (mdi name without the prefix); clients map it.
	Icon string
	// Entity is the Home Assistant entity id (same as ID).
	Entity string
	// State is "on"/"off" for available devices; "" otherwise.
	State string
	// Available is false when HA reports the entity unavailable or unknown.
	Available bool
}

// Catalog is an immutable set of entries in display order.
type Catalog struct {
	entries []Entry
	byID    map[string]int
}

// New wraps already-validated entries (see Build).
func New(entries []Entry) *Catalog {
	c := &Catalog{entries: append([]Entry(nil), entries...), byID: make(map[string]int, len(entries))}
	for i, e := range c.entries {
		c.byID[e.ID] = i
	}
	return c
}

// Domain returns the HA domain of an entity id.
func Domain(entity string) string {
	if i := strings.IndexByte(entity, '.'); i > 0 {
		return entity[:i]
	}
	return ""
}

// Entries returns a copy of the entries in display order.
func (c *Catalog) Entries() []Entry {
	return append([]Entry(nil), c.entries...)
}

// Lookup finds an entry by catalog id.
func (c *Catalog) Lookup(id string) (Entry, bool) {
	i, ok := c.byID[id]
	if !ok {
		return Entry{}, false
	}
	return c.entries[i], true
}

// Actions lists the actions an entry supports.
func (e Entry) Actions() []string {
	if e.Kind == KindScenario {
		return []string{ActionRun}
	}
	return []string{ActionTurnOn, ActionTurnOff}
}

// ServiceCall is a resolved Home Assistant service invocation.
type ServiceCall struct {
	Domain  string
	Service string
	Entity  string
}

// Resolve maps (entry, action) to an HA service call. Unsupported actions are
// rejected here, so the executor can only perform what the catalog allows.
func (e Entry) Resolve(action string) (ServiceCall, error) {
	domain := Domain(e.Entity)
	switch {
	case e.Kind == KindScenario && action == ActionRun:
		// script.turn_on and scene.turn_on both start the entity.
		return ServiceCall{Domain: domain, Service: "turn_on", Entity: e.Entity}, nil
	case e.Kind != KindScenario && (action == ActionTurnOn || action == ActionTurnOff):
		return ServiceCall{Domain: domain, Service: action, Entity: e.Entity}, nil
	}
	return ServiceCall{}, fmt.Errorf("action %q is not supported for %s %q", action, e.Kind, e.ID)
}
