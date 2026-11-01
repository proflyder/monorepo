// Package group defines the extensible tool-group registry of the gateway.
//
// A group is a named bundle of MCP tools. The process enables groups from
// MCP_GROUPS, so adding a new domain means adding a group implementation and
// teaching the registry about it — not changing the transport or auth layer.
package group

import (
	"fmt"
	"sort"
	"strings"

	"github.com/modelcontextprotocol/go-sdk/mcp"
)

// Group is one named bundle of MCP tools.
type Group interface {
	// Name is the env-facing group name, for example "infra.read".
	Name() string
	// Register adds the group's tools to the MCP server.
	Register(s *mcp.Server)
}

// Registry resolves group names from configuration.
type Registry struct {
	groups map[string]Group
}

// NewRegistry builds a registry; duplicate names are a programming error.
func NewRegistry(groups ...Group) *Registry {
	byName := make(map[string]Group, len(groups))
	for _, g := range groups {
		name := strings.TrimSpace(g.Name())
		if name == "" {
			panic("group: empty group name")
		}
		if _, exists := byName[name]; exists {
			panic("group: duplicate group " + name)
		}
		byName[name] = g
	}
	return &Registry{groups: byName}
}

// Names returns every known group name, sorted.
func (r *Registry) Names() []string {
	names := make([]string, 0, len(r.groups))
	for name := range r.groups {
		names = append(names, name)
	}
	sort.Strings(names)
	return names
}

// Select resolves enabled group names from configuration.
func (r *Registry) Select(names []string) ([]Group, error) {
	if len(names) == 0 {
		return nil, fmt.Errorf("no tool groups enabled; set MCP_GROUPS to one or more of %s", strings.Join(r.Names(), ", "))
	}
	seen := make(map[string]bool, len(names))
	var selected []Group
	for _, raw := range names {
		name := strings.TrimSpace(raw)
		if name == "" || seen[name] {
			continue
		}
		g, ok := r.groups[name]
		if !ok {
			return nil, fmt.Errorf("unknown tool group %q; known groups: %s", name, strings.Join(r.Names(), ", "))
		}
		seen[name] = true
		selected = append(selected, g)
	}
	if len(selected) == 0 {
		return nil, fmt.Errorf("no tool groups enabled; set MCP_GROUPS to one or more of %s", strings.Join(r.Names(), ", "))
	}
	return selected, nil
}

// RegisterAll registers every group in order.
func (r *Registry) RegisterAll(s *mcp.Server, groups []Group) {
	for _, g := range groups {
		g.Register(s)
	}
}

// EnabledNames lists the names of selected groups.
func EnabledNames(groups []Group) []string {
	names := make([]string, 0, len(groups))
	for _, g := range groups {
		names = append(names, g.Name())
	}
	return names
}
