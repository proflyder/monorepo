// Package render substitutes `${KEY}` placeholders in the private deploy
// templates.
//
// It is a byte-for-byte port of `projects/home/kubernetes/scheduler/render.py`
// together with the two per-unit wrappers that used to import it
// (`mcp-gateway/render.py`, `wakegrant/render.py`) and the ingress renderer
// embedded in `ingress/deploy.sh`:
//
//   - only placeholders declared by the selected unit's preset may appear;
//   - every declared placeholder must have a non-empty value;
//   - a leftover placeholder is a hard error.
//
// A half-rendered manifest therefore can never reach a cluster. The substitution
// itself is plain text, so the output is byte-identical to the Python renderer.
package render

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"regexp"
	"sort"
	"strings"

	"ricardo.local/monorepo/platform/deploy/internal/buildid"
	"ricardo.local/monorepo/platform/deploy/internal/envfile"
)

// RolloutKey is the placeholder that the `render --rollout-key` flag fills with
// a content fingerprint of the rendered output. Pod templates use it instead of
// the git-based BUILD_ID: a deploy whose rendered content is identical then
// keeps an identical pod template, so `kubectl apply`/`replace` does not create
// a new ReplicaSet and the pods do not restart.
const RolloutKey = "ROLLOUT_ID"

// The two values that must not contribute to the content fingerprint:
// ROLLOUT_ID is the fingerprint itself, and BUILD_ID carries the git revision.
// Both are replaced by fixed markers while hashing, so the fingerprint depends
// only on what the pods run (images, rendered config, mounted ConfigMaps).
const (
	rolloutMask = "rollout-fingerprint"
	buildMask   = "build-traceability"
)

// placeholder matches the templates' `${KEY}` syntax. The character class is
// deliberately narrow (upper-case letters, digits, underscore): a lower-case or
// dotted expression is not a placeholder and is left untouched, exactly like
// the Python regular expression.
var placeholder = regexp.MustCompile(`\$\{([A-Z0-9_]+)\}`)

// schedulerPlaceholders is the allowlist owned by
// `projects/home/kubernetes/scheduler/render.py`. Every deploy unit that uses
// that renderer (scheduler, currency-bot, and the mcp-gateway/wakegrant
// wrappers) starts from this set.
var schedulerPlaceholders = []string{
	"BUILD_ID",
	// Content-addressed rollout annotation: sha256 of the rendered unit, with
	// the git revision and the fingerprint itself masked out. Pod templates
	// reference only this value, so an identical deploy never rolls pods.
	RolloutKey,
	"HA_URL",
	"LAN_CIDR",
	"NODE_LAN_IP",
	"SCHEDULER_NODE_PORT",
	"SCHEDULER_PUBLIC_URL",
	"CONTROL_NODE_PORT",
	// Digest-pinned container images, resolved by deploy.sh from the Bazel
	// build (or from the registry push in CI). Never a moving tag.
	"SCHEDULER_IMAGE",
	"HA_BRIDGE_IMAGE",
	"HA_CONTROL_IMAGE",
	// Pinned image for the in-cluster cutover preflight Job.
	"PREFLIGHT_IMAGE",
	// Currency bot (projects/home/kubernetes/currency-bot) placeholders.
	"CURRENCY_BOT_IMAGE",
	"CLOUDFLARED_IMAGE",
	"CURRENCY_WEBHOOK_URL",
	// LAN NodePort for HA Assist (GET /api/latest), rendered from private env.
	"CURRENCY_BOT_NODE_PORT",
}

// ingressPlaceholders is the key set rendered by the heredoc in
// `ingress/deploy.sh`. That renderer reads exported shell variables instead of
// an env file; the unit preset is the same set so the values can come from
// either source.
var ingressPlaceholders = []string{
	"HOME_DOMAIN",
	"LAN_CIDR",
	"HAOS_ADDRESS",
	"HA_HOST",
	"GRAFANA_HOST",
	"ALARMS_HOST",
	"CONTROL_HOST",
	"CURRENCY_HOST",
}

// Preset is one unit's placeholder policy.
type Preset struct {
	// Name is the `--unit` value.
	Name string
	// Allowed is the substitution allowlist (exact, case-sensitive keys).
	Allowed map[string]bool
	// ContainsLeftover switches the post-render check from "a remaining
	// `${KEY}`" to "any remaining `${`". The ingress heredoc used the latter
	// (it scanned for the two-character prefix), while the Python renderer
	// used a regular expression; keeping both preserves each refusal.
	ContainsLeftover bool
}

func newPreset(name string, keys []string, containsLeftover bool) *Preset {
	allowed := make(map[string]bool, len(keys))
	for _, key := range keys {
		allowed[key] = true
	}
	return &Preset{Name: name, Allowed: allowed, ContainsLeftover: containsLeftover}
}

func withKeys(base *Preset, name string, extra ...string) *Preset {
	keys := make([]string, 0, len(base.Allowed)+len(extra))
	for key := range base.Allowed {
		keys = append(keys, key)
	}
	keys = append(keys, extra...)
	return newPreset(name, keys, false)
}

// PresetFor returns the placeholder policy for a deploy unit.
//
// `scheduler` is the base set. `currency-bot` uses the same renderer and adds
// nothing. `mcp-gateway` and `wakegrant` add their unit-only keys. `ingress`
// is the separate fixed key set from the ingress deploy script.
func PresetFor(unit string) (*Preset, error) {
	base := newPreset("scheduler", schedulerPlaceholders, false)
	switch unit {
	case "", "scheduler":
		return base, nil
	case "currency-bot":
		return newPreset("currency-bot", schedulerPlaceholders, false), nil
	case "mcp-gateway":
		return withKeys(base, "mcp-gateway",
			"MCP_GATEWAY_IMAGE",
			"HAOS_IP",
			"STATION_IP",
			"BB_CT_IP",
			"BB_PROJECT_STATION_LAB",
			"BB_PROJECT_ENGLISH_NOTES",
			"BB_DISPATCHER_THREAD_ID",
		), nil
	case "wakegrant":
		return withKeys(base, "wakegrant",
			"WAKE_GRANT_IMAGE",
			"WAKE_GRANT_NODE_PORT",
		), nil
	case "ingress":
		return newPreset("ingress", ingressPlaceholders, true), nil
	default:
		names := UnitNames()
		return nil, fmt.Errorf("unknown unit %q (expected one of: %s)", unit, strings.Join(names, ", "))
	}
}

// UnitNames lists the supported `--unit` values in a stable order.
func UnitNames() []string {
	names := []string{"scheduler", "currency-bot", "mcp-gateway", "wakegrant", "ingress"}
	sort.Strings(names)
	return names
}

// Render substitutes placeholders in text.
//
// origin is the template path used in error messages. An unknown placeholder, a
// missing/empty value or a leftover placeholder fails the whole render, so a
// caller never receives a partial manifest.
func Render(text string, values map[string]string, origin string, preset *Preset) (string, error) {
	var firstErr error
	rendered := placeholder.ReplaceAllStringFunc(text, func(match string) string {
		if firstErr != nil {
			return match
		}
		key := match[2 : len(match)-1]
		if !preset.Allowed[key] {
			firstErr = fmt.Errorf("%s: placeholder ${%s} is not allowed", origin, key)
			return match
		}
		value, ok := values[key]
		if !ok || value == "" {
			firstErr = fmt.Errorf("%s: missing value for ${%s}", origin, key)
			return match
		}
		// The replacement is literal: `${...}` inside a value is not
		// re-expanded (Python's re.sub does not rescan either).
		return value
	})
	if firstErr != nil {
		return "", firstErr
	}
	if preset.ContainsLeftover {
		if strings.Contains(rendered, "${") {
			return "", fmt.Errorf("unrendered placeholder in %s", origin)
		}
		return rendered, nil
	}
	if leftover := placeholder.FindString(rendered); leftover != "" {
		return "", fmt.Errorf("%s: unrendered placeholder %s", origin, leftover)
	}
	return rendered, nil
}

// Options describes one `render` invocation.
type Options struct {
	// EnvFile is the private KEY=VALUE file. An empty path means "no file";
	// the values then come from Sets only.
	EnvFile string
	// Sets are `KEY=VALUE` overrides applied after the env file, in order.
	Sets []string
	// OutDir receives one file per input template, named after the template.
	OutDir string
	// Files are the templates to render.
	Files []string
	// Unit selects the preset.
	Unit string
	// RolloutKey, when non-empty, is the placeholder replaced by a content
	// fingerprint of the rendered files. Callers pass `RolloutKey` for deploy
	// units whose pod template must only change when the rendered content does.
	RolloutKey string
	// Stdout receives the `rendered <src> -> <dst>` progress lines.
	Stdout io.Writer
}

// renderSource is one template read for a render invocation.
type renderSource struct {
	path string
	raw  string
}

// Run renders every file in opts.
func Run(opts Options) error {
	preset, err := PresetFor(opts.Unit)
	if err != nil {
		return err
	}

	values := make(map[string]string)
	if opts.EnvFile != "" {
		parsed, err := envfile.Parse(opts.EnvFile)
		if err != nil {
			return err
		}
		values = parsed
	}
	for _, item := range opts.Sets {
		key, value, found := strings.Cut(item, "=")
		if !found {
			key, value = item, ""
		}
		if !preset.Allowed[key] {
			return fmt.Errorf("--set %s: not an allowed placeholder", key)
		}
		values[key] = value
	}
	if opts.RolloutKey != "" && !preset.Allowed[opts.RolloutKey] {
		return fmt.Errorf("rollout key %s is not an allowed placeholder", opts.RolloutKey)
	}

	if err := os.MkdirAll(opts.OutDir, 0o777); err != nil {
		return err
	}
	stdout := opts.Stdout
	if stdout == nil {
		stdout = io.Discard
	}

	// Read every source up front: the content fingerprint covers the whole
	// rendered unit, so no file may be written before all of them rendered.
	sources := make([]renderSource, 0, len(opts.Files))
	for _, src := range opts.Files {
		raw, err := os.ReadFile(src)
		if err != nil {
			return err
		}
		sources = append(sources, renderSource{path: src, raw: string(raw)})
	}

	if opts.RolloutKey != "" {
		fingerprint, err := contentFingerprint(sources, values, preset, opts.RolloutKey)
		if err != nil {
			return err
		}
		values[opts.RolloutKey] = fingerprint
		fmt.Fprintf(stdout, "rollout id: %s\n", fingerprint)
	}

	for _, src := range sources {
		rendered, err := Render(src.raw, values, src.path, preset)
		if err != nil {
			return err
		}
		dest := filepath.Join(opts.OutDir, filepath.Base(src.path))
		if err := os.WriteFile(dest, []byte(rendered), 0o644); err != nil {
			return err
		}
		fmt.Fprintf(stdout, "rendered %s -> %s\n", src.path, dest)
	}
	return nil
}

// contentFingerprint hashes the rendered unit with the two values that must not
// feed back into the fingerprint masked: the rollout id itself and BUILD_ID
// (which carries the git revision). The result therefore changes only when the
// rendered content changes: image digests, config values, mounted ConfigMap
// data, probes, resources, and so on.
func contentFingerprint(sources []renderSource, values map[string]string, preset *Preset, rolloutKey string) (string, error) {
	masked := make(map[string]string, len(values))
	for key, value := range values {
		masked[key] = value
	}
	masked[rolloutKey] = rolloutMask
	if _, ok := masked["BUILD_ID"]; ok {
		masked["BUILD_ID"] = buildMask
	}
	var canonical strings.Builder
	for _, src := range sources {
		rendered, err := Render(src.raw, masked, src.path, preset)
		if err != nil {
			return "", err
		}
		canonical.WriteString(filepath.Base(src.path))
		canonical.WriteByte(0)
		canonical.WriteString(rendered)
		canonical.WriteByte(0)
	}
	return buildid.Fingerprint(canonical.String()), nil
}
