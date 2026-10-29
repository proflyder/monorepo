// Package discovery builds the control catalog from Home Assistant itself.
//
// It reads entities (name, icon, state) from /api/states and areas, hidden
// flags and the scenario-role label through read-only template rendering, then
// applies the catalog rules. Results are cached briefly; if Home Assistant becomes
// unreachable the last good catalog is served flagged as stale rather than
// inventing or hardcoding anything.
package discovery

import (
	"context"
	"fmt"
	"log/slog"
	"strings"
	"sync"
	"time"

	"ricardo.local/monorepo/solutions/ha-bridge/internal/catalog"
	"ricardo.local/monorepo/solutions/ha-bridge/internal/ha"
)

// HomeAssistant is the read surface discovery needs.
type HomeAssistant interface {
	Entities(ctx context.Context) ([]ha.Entity, error)
	RenderTemplate(ctx context.Context, template string) (string, error)
}

// Snapshot is a discovered catalog.
type Snapshot struct {
	Catalog *catalog.Catalog
	// StatesOK is false when Home Assistant could not be read and this is the
	// last known catalog; every entry is then reported unavailable.
	StatesOK bool
}

// Provider discovers and caches the catalog.
type Provider struct {
	ha    HomeAssistant
	rules catalog.Rules
	ttl   time.Duration
	log   *slog.Logger
	now   func() time.Time

	mu      sync.Mutex
	last    *catalog.Catalog
	expires time.Time
}

// New builds a provider. ttl bounds how often Home Assistant is queried.
func New(src HomeAssistant, rules catalog.Rules, ttl time.Duration, logger *slog.Logger) *Provider {
	if logger == nil {
		logger = slog.New(slog.DiscardHandler)
	}
	return &Provider{ha: src, rules: rules, ttl: ttl, log: logger, now: time.Now}
}

// Get returns the current catalog, refreshing it when the cache expired.
func (p *Provider) Get(ctx context.Context) (Snapshot, error) {
	p.mu.Lock()
	defer p.mu.Unlock()

	if p.last != nil && p.now().Before(p.expires) {
		return Snapshot{Catalog: p.last, StatesOK: true}, nil
	}
	entries, err := p.discover(ctx)
	if err != nil {
		if p.last == nil {
			return Snapshot{}, err
		}
		p.log.Warn("home assistant discovery failed; serving the last known catalog", "error", err)
		return Snapshot{Catalog: markUnavailable(p.last), StatesOK: false}, nil
	}
	p.last = catalog.New(entries)
	p.expires = p.now().Add(p.ttl)
	return Snapshot{Catalog: p.last, StatesOK: true}, nil
}

func (p *Provider) discover(ctx context.Context) ([]catalog.Entry, error) {
	entities, err := p.ha.Entities(ctx)
	if err != nil {
		return nil, fmt.Errorf("read entities: %w", err)
	}

	// Areas and hidden flags are nice-to-have: without them entries just lose
	// their room / noise filtering. The scenario label decides which scripts are
	// exposed, so when it cannot be read no script is exposed (fail closed);
	// devices and scenes are unaffected.
	rooms := p.rooms(ctx)
	hidden := p.hidden(ctx)
	scenarios := p.scenarios(ctx)

	raws := make([]catalog.Raw, 0, len(entities))
	for _, e := range entities {
		raws = append(raws, catalog.Raw{
			Entity:   e.ID,
			State:    e.State,
			Name:     e.Name,
			Icon:     e.Icon,
			Room:     rooms[e.ID],
			Hidden:   hidden[e.ID],
			Scenario: scenarios[e.ID],
		})
	}
	return catalog.Build(raws, p.rules), nil
}

const domainsList = "['script','scene','light','switch']"

func (p *Provider) rooms(ctx context.Context) map[string]string {
	out, err := p.ha.RenderTemplate(ctx,
		"{% for s in states if s.domain in "+domainsList+" %}{{ s.entity_id }}|{{ area_name(s.entity_id) or '' }}\n{% endfor %}")
	if err != nil {
		p.log.Warn("could not read areas; rooms will be empty", "error", err)
		return nil
	}
	rooms := map[string]string{}
	for _, line := range strings.Split(out, "\n") {
		id, room, ok := strings.Cut(strings.TrimSpace(line), "|")
		if ok && room != "" && room != "None" {
			rooms[id] = room
		}
	}
	return rooms
}

func (p *Provider) hidden(ctx context.Context) map[string]bool {
	out, err := p.ha.RenderTemplate(ctx,
		"{% for s in states if s.domain in "+domainsList+" and is_hidden_entity(s.entity_id) %}{{ s.entity_id }}\n{% endfor %}")
	if err != nil {
		p.log.Warn("could not read hidden flags; hidden entities will not be filtered", "error", err)
		return nil
	}
	return lines(out)
}

func (p *Provider) scenarios(ctx context.Context) map[string]bool {
	// Rules.Validate guarantees the label is [a-z0-9_-], so it is safe to inline.
	out, err := p.ha.RenderTemplate(ctx, "{{ label_entities('"+p.rules.ScenarioLabel+"') | join('\\n') }}")
	if err != nil {
		p.log.Warn("could not read the scenario label; no scripts will be exposed",
			"label", p.rules.ScenarioLabel, "error", err)
		return nil
	}
	return lines(out)
}

func lines(text string) map[string]bool {
	set := map[string]bool{}
	for _, line := range strings.Split(text, "\n") {
		if line = strings.TrimSpace(line); line != "" {
			set[line] = true
		}
	}
	return set
}

func markUnavailable(c *catalog.Catalog) *catalog.Catalog {
	entries := c.Entries()
	for i := range entries {
		entries[i].Available = false
		entries[i].State = ""
	}
	return catalog.New(entries)
}
