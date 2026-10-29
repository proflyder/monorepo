// Package catalog describes the Home Assistant entities the phone may control.
//
// The catalog is deployment configuration (a JSON file), never code. It is an
// allowlist: only listed entities can be shown or driven, and the executor
// resolves every command through it, so neither the phone nor a task payload
// can choose a Home Assistant service or entity.
package catalog

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"regexp"
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

const (
	maxEntries = 200
	maxText    = 80
)

var (
	idPattern     = regexp.MustCompile(`^[a-z0-9][a-z0-9._-]{0,63}$`)
	entityPattern = regexp.MustCompile(`^[a-z0-9_]+\.[a-z0-9_]+$`)
	iconPattern   = regexp.MustCompile(`^[a-z0-9_-]{1,32}$`)
)

// Entry is one controllable thing.
type Entry struct {
	// ID is the stable catalog id the phone sends back; it is not an HA id.
	ID    string `json:"id"`
	Kind  Kind   `json:"kind"`
	Title string `json:"title"`
	// Room groups devices and scenarios in the UI. Optional.
	Room string `json:"room,omitempty"`
	// Icon is a hint ("work", "moon", "bulb", ...); clients map it to artwork.
	Icon string `json:"icon,omitempty"`
	// Entity is the Home Assistant entity id, e.g. "script.blackout".
	Entity string `json:"entity"`
}

// Catalog is a validated, immutable set of entries in display order.
type Catalog struct {
	entries []Entry
	byID    map[string]int
}

type fileFormat struct {
	Version int     `json:"version"`
	Entries []Entry `json:"entries"`
}

// Load reads and validates a catalog file.
func Load(path string) (*Catalog, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("catalog: read file: %w", err)
	}
	return Parse(raw)
}

// Parse validates catalog JSON.
func Parse(raw []byte) (*Catalog, error) {
	var file fileFormat
	dec := json.NewDecoder(strings.NewReader(string(raw)))
	dec.DisallowUnknownFields()
	if err := dec.Decode(&file); err != nil {
		return nil, fmt.Errorf("catalog: invalid JSON: %w", err)
	}
	if file.Version != 1 {
		return nil, fmt.Errorf("catalog: unsupported version %d (want 1)", file.Version)
	}
	if len(file.Entries) == 0 {
		return nil, errors.New("catalog: no entries")
	}
	if len(file.Entries) > maxEntries {
		return nil, fmt.Errorf("catalog: more than %d entries", maxEntries)
	}

	c := &Catalog{byID: make(map[string]int, len(file.Entries))}
	entities := make(map[string]bool, len(file.Entries))
	for i, e := range file.Entries {
		e.Title = strings.TrimSpace(e.Title)
		e.Room = strings.TrimSpace(e.Room)
		if err := validate(e); err != nil {
			return nil, fmt.Errorf("catalog: entry %d (%q): %w", i, e.ID, err)
		}
		if _, dup := c.byID[e.ID]; dup {
			return nil, fmt.Errorf("catalog: duplicate id %q", e.ID)
		}
		if entities[e.Entity] {
			return nil, fmt.Errorf("catalog: entity %q listed twice", e.Entity)
		}
		entities[e.Entity] = true
		c.byID[e.ID] = len(c.entries)
		c.entries = append(c.entries, e)
	}
	return c, nil
}

func validate(e Entry) error {
	if !idPattern.MatchString(e.ID) {
		return errors.New("id must match [a-z0-9][a-z0-9._-]{0,63}")
	}
	if e.Title == "" || len(e.Title) > maxText {
		return fmt.Errorf("title must be 1..%d bytes", maxText)
	}
	if len(e.Room) > maxText {
		return fmt.Errorf("room must be at most %d bytes", maxText)
	}
	if e.Icon != "" && !iconPattern.MatchString(e.Icon) {
		return errors.New("icon must match [a-z0-9_-]{1,32}")
	}
	if !entityPattern.MatchString(e.Entity) {
		return errors.New("entity must look like domain.object_id")
	}
	domain := Domain(e.Entity)
	switch e.Kind {
	case KindScenario:
		if domain != "script" && domain != "scene" {
			return errors.New("scenario entity must be a script.* or scene.*")
		}
	case KindLight:
		if domain != "light" {
			return errors.New("light entity must be a light.*")
		}
	case KindSwitch:
		if domain != "switch" {
			return errors.New("switch entity must be a switch.*")
		}
	default:
		return fmt.Errorf("unknown kind %q", e.Kind)
	}
	return nil
}

// Domain returns the HA domain of an entity id.
func Domain(entity string) string {
	if i := strings.IndexByte(entity, '.'); i > 0 {
		return entity[:i]
	}
	return ""
}

// Entries returns a copy of the entries in file order.
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
