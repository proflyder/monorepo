package catalog

import (
	"errors"
	"path"
	"regexp"
	"sort"
	"strings"
)

// Raw is one discovered Home Assistant entity before filtering.
type Raw struct {
	Entity string
	State  string
	Name   string
	Icon   string
	Room   string
	// Hidden mirrors HA's "hidden entity" flag: the user already decided it is noise.
	Hidden bool
	// Labeled is true when the entity carries Rules.Label.
	Labeled bool
}

// Rules decide what discovery exposes. There is no entity list anywhere:
// the source of truth is Home Assistant (areas, names, labels, hidden flags).
type Rules struct {
	// Label is an optional HA label id. When set, lights, switches and scenes
	// are exposed only if they carry it (an explicit allowlist); when empty
	// they are all exposed. Scripts follow their own rule, see Build.
	Label string
	// Exclude are path.Match globs on the entity id (e.g. "switch.*_child_lock").
	Exclude []string
}

const (
	maxEntries = 200
	maxText    = 80
)

var (
	entityPattern = regexp.MustCompile(`^[a-z0-9_]+\.[a-z0-9_]+$`)
	iconPattern   = regexp.MustCompile(`[^a-z0-9_-]+`)
	labelPattern  = regexp.MustCompile(`^[a-z0-9_-]{1,64}$`)
)

// Validate checks rule syntax so a misconfiguration fails at startup.
func (r Rules) Validate() error {
	if r.Label != "" && !labelPattern.MatchString(r.Label) {
		return errors.New("label must be an HA label id: [a-z0-9_-]{1,64}")
	}
	for _, g := range r.Exclude {
		if _, err := path.Match(g, ""); err != nil {
			return errors.New("invalid exclude pattern " + g)
		}
	}
	return nil
}

func kindOf(domain string) (Kind, bool) {
	switch domain {
	case "script", "scene":
		return KindScenario, true
	case "light":
		return KindLight, true
	case "switch":
		return KindSwitch, true
	}
	return "", false
}

func (r Rules) excluded(entity string) bool {
	for _, g := range r.Exclude {
		if ok, _ := path.Match(g, entity); ok {
			return true
		}
	}
	return false
}

// Build applies the rules to discovered entities. Output order is stable:
// scenarios, then devices; within each, by room (unassigned last), title, id.
func Build(raws []Raw, rules Rules) []Entry {
	entries := make([]Entry, 0, len(raws))
	for _, raw := range raws {
		if !entityPattern.MatchString(raw.Entity) {
			continue
		}
		domain := Domain(raw.Entity)
		kind, ok := kindOf(domain)
		if !ok || raw.Hidden || rules.excluded(raw.Entity) {
			continue
		}
		if domain == "script" {
			// Most scripts are internals (alarm helpers, RPC wrappers). A script
			// is a user-facing scenario when it says so in Home Assistant: it
			// has an icon, or carries the label. This works for YAML scripts,
			// which cannot be labeled (they have no entity-registry entry).
			if !raw.Labeled && strings.TrimSpace(raw.Icon) == "" {
				continue
			}
		} else if rules.Label != "" && !raw.Labeled {
			continue
		}

		state := strings.ToLower(strings.TrimSpace(raw.State))
		available := state != "unavailable" && state != "unknown" && state != ""
		entry := Entry{
			ID:        raw.Entity,
			Kind:      kind,
			Title:     titleOf(raw),
			Room:      truncate(strings.TrimSpace(raw.Room)),
			Icon:      iconOf(raw.Icon),
			Entity:    raw.Entity,
			Available: available,
		}
		if kind != KindScenario && available && (state == "on" || state == "off") {
			entry.State = state
		}
		entries = append(entries, entry)
	}

	sort.SliceStable(entries, func(i, j int) bool {
		a, b := entries[i], entries[j]
		if (a.Kind == KindScenario) != (b.Kind == KindScenario) {
			return a.Kind == KindScenario
		}
		if (a.Room == "") != (b.Room == "") {
			return b.Room == ""
		}
		if a.Room != b.Room {
			return a.Room < b.Room
		}
		if a.Title != b.Title {
			return strings.ToLower(a.Title) < strings.ToLower(b.Title)
		}
		return a.ID < b.ID
	})
	if len(entries) > maxEntries {
		entries = entries[:maxEntries]
	}
	return entries
}

func titleOf(raw Raw) string {
	if name := truncate(strings.TrimSpace(raw.Name)); name != "" {
		return name
	}
	// "light.bedside_lamp" -> "Bedside lamp"
	object := raw.Entity[strings.IndexByte(raw.Entity, '.')+1:]
	object = strings.ReplaceAll(object, "_", " ")
	if object == "" {
		return raw.Entity
	}
	return strings.ToUpper(object[:1]) + object[1:]
}

func iconOf(icon string) string {
	icon = iconPattern.ReplaceAllString(strings.ToLower(strings.TrimPrefix(strings.TrimSpace(icon), "mdi:")), "")
	if len(icon) > 32 {
		icon = icon[:32]
	}
	return icon
}

func truncate(text string) string {
	if len(text) > maxText {
		return text[:maxText]
	}
	return text
}
