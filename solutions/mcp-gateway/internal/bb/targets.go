package bb

import (
	"errors"
	"fmt"
	"os"
	"strings"
	"unicode"

	"go.yaml.in/yaml/v2"
)

// DefaultProvider is used when a target entry does not pin one. Hermes is
// personal-only and the personal profile is expected to allow Claude Code.
const DefaultProvider = "claude-code"

var (
	// ErrTargetNotFound covers both an unknown alias and a project that is not
	// a personal target. The two cases are deliberately indistinguishable so
	// Hermes can never learn that a work project exists.
	ErrTargetNotFound = errors.New("bb: target not found")
	// ErrProviderUnavailable means the project does not expose the provider.
	ErrProviderUnavailable = errors.New("bb: provider unavailable")
	// ErrModelNotFound means the requested model is not allowed by the plugin.
	ErrModelNotFound = errors.New("bb: model not found")
	// ErrEffortNotSupported means the model does not support the level.
	ErrEffortNotSupported = errors.New("bb: reasoning level not supported")
)

// TargetsFile is the tracked allowlist of spoken project names.
type TargetsFile struct {
	Targets []TargetEntry `yaml:"targets"`
}

// TargetEntry maps spoken aliases to one project and its optional pinning.
type TargetEntry struct {
	// Project is the plugin projectId or its exact name. Deployment renders it
	// from private values; the tracked file only carries a placeholder.
	Project string `yaml:"project"`
	// Aliases are the spoken names; at least one is required.
	Aliases []string `yaml:"aliases"`
	// Provider/Model/Effort may pin the selection. Empty means "first allowed
	// by the plugin".
	Provider string `yaml:"provider,omitempty"`
	Model    string `yaml:"model,omitempty"`
	Effort   string `yaml:"effort,omitempty"`
	// TitlePrefix labels created threads, e.g. "Голос".
	TitlePrefix string `yaml:"title_prefix,omitempty"`
}

// Resolver resolves a spoken project name to a personal target.
type Resolver struct {
	entries         []TargetEntry
	defaultProvider string
}

// LoadTargetsFile reads and validates the allowlist.
func LoadTargetsFile(path string) (*Resolver, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("bb targets: %w", err)
	}
	return ParseTargets(data)
}

// ParseTargets validates an allowlist document.
func ParseTargets(data []byte) (*Resolver, error) {
	var file TargetsFile
	if err := yaml.Unmarshal(data, &file); err != nil {
		return nil, fmt.Errorf("bb targets: %w", err)
	}
	if len(file.Targets) == 0 {
		return nil, errors.New("bb targets: at least one target is required")
	}
	seen := make(map[string]string, len(file.Targets))
	entries := make([]TargetEntry, 0, len(file.Targets))
	for _, entry := range file.Targets {
		entry.Project = strings.TrimSpace(entry.Project)
		entry.Provider = strings.TrimSpace(entry.Provider)
		entry.Model = strings.TrimSpace(entry.Model)
		entry.Effort = strings.TrimSpace(entry.Effort)
		entry.TitlePrefix = strings.TrimSpace(entry.TitlePrefix)
		if entry.Project == "" {
			return nil, errors.New("bb targets: project is required")
		}
		if len(entry.Aliases) == 0 {
			return nil, fmt.Errorf("bb targets: %q has no aliases", entry.Project)
		}
		aliases := make([]string, 0, len(entry.Aliases))
		for _, alias := range entry.Aliases {
			normalized := normalize(alias)
			if normalized == "" {
				return nil, fmt.Errorf("bb targets: %q has an empty alias", entry.Project)
			}
			if other, exists := seen[normalized]; exists {
				return nil, fmt.Errorf("bb targets: alias %q is used by %q and %q", alias, other, entry.Project)
			}
			seen[normalized] = entry.Project
			aliases = append(aliases, strings.TrimSpace(alias))
		}
		entry.Aliases = aliases
		entries = append(entries, entry)
	}
	return &Resolver{entries: entries, defaultProvider: DefaultProvider}, nil
}

// Entries returns a copy of the parsed allowlist (for logs/tests).
func (r *Resolver) Entries() []TargetEntry {
	out := make([]TargetEntry, len(r.entries))
	copy(out, r.entries)
	return out
}

// Resolution is a fully validated spawn target.
type Resolution struct {
	Project     string
	ProjectID   string
	ProviderID  string
	Model       string
	Effort      string
	TitlePrefix string
}

// Resolve maps a spoken project name onto a personal target from the plugin's
// live target list. model/effort override the config pinning; both must still
// be allowed by the plugin.
func (r *Resolver) Resolve(targets Targets, spoken, model, effort string) (Resolution, error) {
	normalized := normalize(spoken)
	if normalized == "" {
		return Resolution{}, ErrTargetNotFound
	}
	entry, ok := r.matchEntry(normalized)
	if !ok {
		// Allow the plugin's project name itself as a spoken alias, but only
		// for personal targets.
		entry = TargetEntry{}
	}
	target, ok := findPersonal(targets, entry, normalized)
	if !ok {
		return Resolution{}, ErrTargetNotFound
	}
	if entry.Project == "" {
		entry.Project = target.Name
	}

	providerID := strings.TrimSpace(entry.Provider)
	if providerID == "" {
		providerID = r.defaultProvider
	}
	if override := strings.TrimSpace(model); override != "" {
		entry.Model = override
	}
	if override := strings.TrimSpace(effort); override != "" {
		entry.Effort = override
	}

	provider, ok := findProvider(target, providerID)
	if !ok {
		return Resolution{}, ErrProviderUnavailable
	}
	if len(provider.Models) == 0 {
		return Resolution{}, ErrProviderUnavailable
	}
	selected := provider.Models[0]
	if entry.Model != "" {
		found := false
		for _, candidate := range provider.Models {
			if candidate.Model == entry.Model {
				selected = candidate
				found = true
				break
			}
		}
		if !found {
			return Resolution{}, ErrModelNotFound
		}
	}
	level := strings.TrimSpace(entry.Effort)
	if level == "" {
		level = selected.DefaultReasoningLevel
	}
	if level == "" && len(selected.ReasoningLevels) > 0 {
		level = selected.ReasoningLevels[0]
	}
	if level != "" && len(selected.ReasoningLevels) > 0 && !contains(selected.ReasoningLevels, level) {
		return Resolution{}, ErrEffortNotSupported
	}

	return Resolution{
		Project:     target.Name,
		ProjectID:   target.ProjectID,
		ProviderID:  provider.ProviderID,
		Model:       selected.Model,
		Effort:      level,
		TitlePrefix: entry.TitlePrefix,
	}, nil
}

func (r *Resolver) matchEntry(normalized string) (TargetEntry, bool) {
	for _, entry := range r.entries {
		for _, alias := range entry.Aliases {
			if normalize(alias) == normalized {
				return entry, true
			}
		}
	}
	return TargetEntry{}, false
}

// findPersonal returns the personal target addressed by the configured entry or
// by the spoken project name. A project that exists only in another profile is
// reported as not found.
func findPersonal(targets Targets, entry TargetEntry, normalized string) (Target, bool) {
	wanted := normalize(entry.Project)
	for _, target := range targets.Projects {
		if target.Profile != ProfilePersonal {
			continue
		}
		if entry.Project != "" {
			if strings.TrimSpace(target.ProjectID) == strings.TrimSpace(entry.Project) ||
				normalize(target.Name) == wanted {
				return target, true
			}
			continue
		}
		if normalize(target.Name) == normalized {
			return target, true
		}
	}
	return Target{}, false
}

func findProvider(target Target, providerID string) (Provider, bool) {
	for _, provider := range target.Providers {
		if provider.ProviderID == providerID {
			return provider, true
		}
	}
	return Provider{}, false
}

func contains(values []string, want string) bool {
	for _, value := range values {
		if value == want {
			return true
		}
	}
	return false
}

// normalize mirrors the catalog's spoken-name normalization: lower case,
// ё→е, punctuation and repeated spaces collapsed.
func normalize(value string) string {
	var b strings.Builder
	space := false
	for _, r := range strings.ToLower(strings.TrimSpace(value)) {
		if r == 'ё' {
			r = 'е'
		}
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			if space && b.Len() > 0 {
				b.WriteByte(' ')
			}
			space = false
			b.WriteRune(r)
			continue
		}
		space = true
	}
	return strings.TrimSpace(b.String())
}
