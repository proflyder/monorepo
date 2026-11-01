// Package catalog loads and validates the infra service catalog.
//
// The catalog is deployment data, never solution data: it is a YAML file whose
// path comes from the environment. Nothing here knows about a concrete home
// project, namespace or endpoint.
package catalog

import (
	"errors"
	"fmt"
	"os"
	"sort"
	"strings"
	"unicode"

	"go.yaml.in/yaml/v2"
)

// Supported workload kinds. The only mutation the gateway performs is a
// rollout restart, so these three are the complete set.
const (
	KindDeployment  = "Deployment"
	KindStatefulSet = "StatefulSet"
	KindDaemonSet   = "DaemonSet"
)

// Service kinds.
const (
	ServiceK8s      = "k8s"
	ServiceExternal = "external"
)

// Log sources.
const (
	LogsLoki = "loki"
	LogsPods = "pods"
	LogsNone = "none"
)

// Catalog is the whole catalog file.
type Catalog struct {
	// Overview is sanitized free-form markdown about the topology. It is served
	// as the infra://catalog MCP resource and returned by infra_overview.
	Overview string `yaml:"overview"`
	// Services is the allow-list of things the gateway may read and (when
	// restart.allowed) restart.
	Services []Service `yaml:"services"`
}

// Aliases are the spoken names for a service. Both languages are optional but
// at least one alias across both lists is required.
type Aliases struct {
	RU []string `yaml:"ru"`
	EN []string `yaml:"en"`
}

// Workload identifies one Kubernetes workload.
type Workload struct {
	Kind string `yaml:"kind"`
	Name string `yaml:"name"`
}

// Signal is one Prometheus query used as a health signal for the service.
type Signal struct {
	// PromQL is an instant query. A non-empty, non-zero result means "problem"
	// for signals with Status set, and is shown as-is otherwise.
	PromQL string `yaml:"promql"`
	// Meaning is the short human explanation spoken in a verdict.
	Meaning string `yaml:"meaning"`
	// Status includes this signal in the infra_status aggregate.
	Status bool `yaml:"status"`
}

// LogSource describes where logs for the service come from.
type LogSource struct {
	// Source is one of loki, pods, none.
	Source string `yaml:"source"`
	// Selector is the LogQL selector used when Source is loki.
	Selector string `yaml:"selector"`
	// Containers restricts pods/log collection to these container names.
	Containers []string `yaml:"containers"`
}

// Restart is the restart policy for the service.
type Restart struct {
	// Allowed enables infra_prepare_restart/infra_confirm for this service.
	Allowed bool `yaml:"allowed"`
	// SelfDisrupting defers the restart so the reply can be spoken first.
	SelfDisrupting bool `yaml:"self_disrupting"`
	// Warning is spoken inside the confirmation question.
	Warning string `yaml:"warning"`
}

// Service is one catalog entry.
type Service struct {
	ID          string    `yaml:"id"`
	Aliases     Aliases   `yaml:"aliases"`
	Kind        string    `yaml:"kind"`
	Namespace   string    `yaml:"namespace"`
	Workload    Workload  `yaml:"workload"`
	Containers  []string  `yaml:"containers"`
	Signals     []Signal  `yaml:"signals"`
	Logs        LogSource `yaml:"logs"`
	Restart     Restart   `yaml:"restart"`
	Description string    `yaml:"description"`
	AliasIndex  []string  `yaml:"-"`
	Normalized  []string  `yaml:"-"`
}

// LoadFile reads and validates a catalog from path.
func LoadFile(path string) (*Catalog, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("read catalog: %w", err)
	}
	return Parse(data)
}

// Parse decodes and validates catalog bytes.
func Parse(data []byte) (*Catalog, error) {
	var cat Catalog
	if err := yaml.Unmarshal(data, &cat); err != nil {
		return nil, fmt.Errorf("parse catalog: %w", err)
	}
	if err := cat.Validate(); err != nil {
		return nil, err
	}
	cat.prepare()
	return &cat, nil
}

// Validate returns every schema problem it can find, joined into one error.
func (c *Catalog) Validate() error {
	var problems []string
	if len(c.Services) == 0 {
		problems = append(problems, "catalog has no services")
	}
	seen := make(map[string]int, len(c.Services))
	for i := range c.Services {
		s := &c.Services[i]
		prefix := fmt.Sprintf("services[%d]", i)
		if s.ID == "" {
			problems = append(problems, prefix+": id is required")
			prefix = "services[" + fmt.Sprint(i) + "]"
		} else {
			prefix = "service " + s.ID
			if first, ok := seen[s.ID]; ok {
				problems = append(problems, fmt.Sprintf("%s: duplicate id (first at services[%d])", prefix, first))
			} else {
				seen[s.ID] = i
			}
		}
		if len(s.Aliases.RU)+len(s.Aliases.EN) == 0 {
			problems = append(problems, prefix+": at least one ru or en alias is required")
		}
		for _, alias := range append(append([]string(nil), s.Aliases.RU...), s.Aliases.EN...) {
			if strings.TrimSpace(alias) == "" {
				problems = append(problems, prefix+": aliases must not contain empty entries")
				break
			}
		}
		switch s.Kind {
		case ServiceK8s:
			if s.Namespace == "" {
				problems = append(problems, prefix+": namespace is required for kind k8s")
			}
			switch s.Workload.Kind {
			case KindDeployment, KindStatefulSet, KindDaemonSet:
			default:
				problems = append(problems, fmt.Sprintf(
					"%s: workload.kind must be Deployment, StatefulSet or DaemonSet (got %q)",
					prefix, s.Workload.Kind))
			}
			if s.Workload.Name == "" {
				problems = append(problems, prefix+": workload.name is required for kind k8s")
			}
		case ServiceExternal:
			if s.Workload.Name != "" || s.Namespace != "" {
				problems = append(problems, prefix+": external services must not declare a workload or namespace")
			}
			if s.Restart.Allowed {
				problems = append(problems, prefix+": external services cannot allow restart")
			}
			if s.Logs.Source == LogsPods {
				problems = append(problems, prefix+": external services cannot read pods/log")
			}
		default:
			problems = append(problems, fmt.Sprintf("%s: kind must be k8s or external (got %q)", prefix, s.Kind))
		}
		for j, sig := range s.Signals {
			if strings.TrimSpace(sig.PromQL) == "" {
				problems = append(problems, fmt.Sprintf("%s: signals[%d].promql is required", prefix, j))
			}
			if strings.TrimSpace(sig.Meaning) == "" {
				problems = append(problems, fmt.Sprintf("%s: signals[%d].meaning is required", prefix, j))
			}
		}
		switch s.Logs.Source {
		case "", LogsNone:
		case LogsLoki:
			if strings.TrimSpace(s.Logs.Selector) == "" {
				problems = append(problems, prefix+": logs.selector is required for source loki")
			}
		case LogsPods:
			if s.Kind != ServiceK8s {
				problems = append(problems, prefix+": logs.source pods requires kind k8s")
			}
		default:
			problems = append(problems, fmt.Sprintf("%s: logs.source must be loki, pods or none (got %q)", prefix, s.Logs.Source))
		}
		if s.Restart.Allowed && strings.TrimSpace(s.Restart.Warning) == "" {
			problems = append(problems, prefix+": restart.warning is required when restart is allowed")
		}
	}
	if len(problems) == 0 {
		return nil
	}
	sort.Strings(problems)
	return errors.New("invalid catalog: " + strings.Join(problems, "; "))
}

// prepare builds alias lookup structures. Call after Validate.
func (c *Catalog) prepare() {
	for i := range c.Services {
		s := &c.Services[i]
		s.AliasIndex = append(append([]string(nil), s.Aliases.RU...), s.Aliases.EN...)
		s.Normalized = s.Normalized[:0]
		for _, alias := range s.AliasIndex {
			s.Normalized = append(s.Normalized, Normalize(alias))
		}
	}
}

// Find returns the service with the given id.
func (c *Catalog) Find(id string) (Service, bool) {
	for _, s := range c.Services {
		if s.ID == id {
			return s, true
		}
	}
	return Service{}, false
}

// Resolve maps a spoken name to exactly one service.
//
// Matching is tolerant: case, ё/е and punctuation are ignored, then an exact
// normalized match wins, then a unique substring match, then a unique
// one-edit-distance match for names long enough to make that safe.
func (c *Catalog) Resolve(name string) (Service, error) {
	query := Normalize(name)
	if query == "" {
		return Service{}, errors.New("empty service name")
	}
	for _, s := range c.Services {
		for _, normalized := range s.Normalized {
			if normalized == query {
				return s, nil
			}
		}
	}
	if s, ok := uniqueMatch(c.Services, func(s *Service) bool {
		for _, normalized := range s.Normalized {
			if strings.Contains(normalized, query) || strings.Contains(query, normalized) {
				return true
			}
		}
		return false
	}); ok {
		return s, nil
	}
	if len([]rune(query)) >= 5 {
		if s, ok := uniqueMatch(c.Services, func(s *Service) bool {
			for _, normalized := range s.Normalized {
				if withinOneEdit(query, normalized) {
					return true
				}
			}
			return false
		}); ok {
			return s, nil
		}
	}
	return Service{}, fmt.Errorf("unknown service %q", strings.TrimSpace(name))
}

func uniqueMatch(services []Service, match func(*Service) bool) (Service, bool) {
	var found []Service
	for i := range services {
		if match(&services[i]) {
			found = append(found, services[i])
		}
	}
	if len(found) != 1 {
		return Service{}, false
	}
	return found[0], true
}

// Normalize lowercases a spoken name, folds ё to е and replaces any run of
// non-alphanumeric runes with a single space.
func Normalize(raw string) string {
	var b strings.Builder
	b.Grow(len(raw))
	space := true
	for _, r := range strings.ToLower(strings.TrimSpace(raw)) {
		switch r {
		case 'ё':
			r = 'е'
		}
		if unicode.IsLetter(r) || unicode.IsDigit(r) {
			b.WriteRune(r)
			space = false
			continue
		}
		if !space {
			b.WriteByte(' ')
			space = true
		}
	}
	return strings.TrimSpace(b.String())
}

func withinOneEdit(a, b string) bool {
	ar, br := []rune(a), []rune(b)
	if abs(len(ar)-len(br)) > 1 {
		return false
	}
	if len(ar) > len(br) {
		ar, br = br, ar
	}
	i, j := 0, 0
	edits := 0
	for i < len(ar) && j < len(br) {
		if ar[i] == br[j] {
			i++
			j++
			continue
		}
		edits++
		if edits > 1 {
			return false
		}
		if len(ar) == len(br) {
			i++
			j++
		} else {
			j++
		}
	}
	return edits+len(ar)-i+len(br)-j <= 1
}

func abs(n int) int {
	if n < 0 {
		return -n
	}
	return n
}

// OverviewText returns the overview markdown, or a short fallback.
func (c *Catalog) OverviewText() string {
	if strings.TrimSpace(c.Overview) != "" {
		return c.Overview
	}
	return "Каталог сервисов без описания топологии."
}

// CatalogText renders the catalog as human-readable markdown for the model.
func (c *Catalog) CatalogText() string {
	var b strings.Builder
	b.WriteString("# Каталог сервисов\n\n")
	for _, s := range c.Services {
		aliases := append(append([]string(nil), s.Aliases.RU...), s.Aliases.EN...)
		fmt.Fprintf(&b, "## %s\n", s.ID)
		if s.Description != "" {
			fmt.Fprintf(&b, "%s\n", s.Description)
		}
		fmt.Fprintf(&b, "- aliases: %s\n", strings.Join(aliases, ", "))
		if s.Kind == ServiceK8s {
			fmt.Fprintf(&b, "- workload: %s %s/%s\n", s.Workload.Kind, s.Namespace, s.Workload.Name)
		} else {
			b.WriteString("- workload: external (read-only signals)\n")
		}
		if s.Restart.Allowed {
			fmt.Fprintf(&b, "- restart: allowed; warning: %s\n", s.Restart.Warning)
			if s.Restart.SelfDisrupting {
				b.WriteString("- restart is deferred (self-disrupting)\n")
			}
		} else {
			b.WriteString("- restart: not allowed\n")
		}
		for _, sig := range s.Signals {
			fmt.Fprintf(&b, "- signal (%s): %s\n", sig.Meaning, sig.PromQL)
		}
		b.WriteString("\n")
	}
	return b.String()
}

// K8sServices returns only the Kubernetes-backed services.
func (c *Catalog) K8sServices() []Service {
	out := make([]Service, 0, len(c.Services))
	for _, s := range c.Services {
		if s.Kind == ServiceK8s {
			out = append(out, s)
		}
	}
	return out
}
