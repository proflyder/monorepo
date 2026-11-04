// Package hermesconfig renders the reviewed Hermes overlay into a live
// HERMES_HOME.
//
// It is the Go port of `projects/home-assistant/hermes/render_hermes_config.py`:
// the same merge rules (fragment, then private values, then the voice prompt),
// the same TEST-NET/example placeholder refusal, the same `.prev` backups and
// the same `--dry-run` / `--check` modes. The YAML document is held as an
// ordered tree (`yaml.MapSlice`), so key order is preserved exactly like the
// Python renderer.
//
// Emitter style is not byte-identical to PyYAML in every case (go-yaml folds
// long scalars at 80 columns and double-quotes numeric-looking strings where
// PyYAML uses single quotes), but the parsed document is identical. DumpConfig
// removes the folding where that is provably lossless, and the golden test
// compares parsed trees so a style difference can never hide a value change.
package hermesconfig

import (
	"bytes"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"reflect"
	"regexp"
	"sort"
	"strings"
	"time"

	yaml "go.yaml.in/yaml/v2"
)

// Installed files are copied verbatim into HERMES_HOME.
var installed = []struct {
	name   string
	source func(Options) string
}{
	{".hermes.md", func(o Options) string { return o.Context }},
	{"SOUL.md", func(o Options) string { return o.Soul }},
}

// privateOnlyKeys configure the render and are removed before the config is
// written, so they can never leak into Hermes' own config.yaml.
var privateOnlyKeys = []string{"telegram_owner_chat_id"}

var (
	placeholderPattern = regexp.MustCompile(`(?i)\b192\.0\.2\.\d{1,3}\b|\bexample\.(?:com|net|org)\b`)
	secretLinePattern  = regexp.MustCompile(`(?i)((?:^|\b)(?:api[_-]?key|token|secret|password|authorization|bearer)\b\s*[:=]\s*)(?:"[^"]*"|'[^']*'|\S+)`)
)

// Options describes one render.
type Options struct {
	Home              string
	Fragment          string
	Context           string
	Soul              string
	VoicePrompt       string
	TelegramPrompt    string
	Skills            string
	PrivateValues     string // empty means "no private values"
	AllowPlaceholders bool
}

// File is one planned write.
type File struct {
	Path    string
	Content string
}

// DeepMerge merges overlay into base: mappings merge, everything else replaces.
// The inputs are not mutated.
func DeepMerge(base, overlay interface{}) interface{} {
	baseMap, baseOK := base.(yaml.MapSlice)
	overlayMap, overlayOK := overlay.(yaml.MapSlice)
	if !baseOK || !overlayOK {
		return overlay
	}
	merged := make(yaml.MapSlice, len(baseMap), len(baseMap)+len(overlayMap))
	copy(merged, baseMap)
	for _, item := range overlayMap {
		index := indexOfKey(merged, item.Key)
		if index >= 0 {
			merged[index].Value = DeepMerge(merged[index].Value, item.Value)
		} else {
			merged = append(merged, yaml.MapItem{Key: item.Key, Value: item.Value})
		}
	}
	return merged
}

// IterLeaves yields (dotted path, value) for every non-mapping leaf.
func IterLeaves(data interface{}, prefix []string) [][2]interface{} {
	var leaves [][2]interface{}
	var walk func(interface{}, []string)
	walk = func(node interface{}, path []string) {
		if mapping, ok := node.(yaml.MapSlice); ok {
			for _, item := range mapping {
				walk(item.Value, append(append([]string(nil), path...), fmt.Sprint(item.Key)))
			}
			return
		}
		leaves = append(leaves, [2]interface{}{strings.Join(path, "."), node})
	}
	walk(data, prefix)
	return leaves
}

// PlaceholderViolations returns the keys whose placeholder value differs from
// (or is absent in) the live config.
func PlaceholderViolations(merged, live interface{}) []string {
	current := make(map[string]interface{})
	for _, leaf := range IterLeaves(live, nil) {
		current[leaf[0].(string)] = leaf[1]
	}
	bad := make(map[string]struct{})
	for _, leaf := range IterLeaves(merged, nil) {
		path := leaf[0].(string)
		value, ok := leaf[1].(string)
		if !ok || !placeholderPattern.MatchString(value) {
			continue
		}
		if currentPath, present := current[path]; !present || !reflect.DeepEqual(currentPath, value) {
			bad[path] = struct{}{}
		}
	}
	violations := make([]string, 0, len(bad))
	for path := range bad {
		violations = append(violations, path)
	}
	sort.Strings(violations)
	return violations
}

// DumpConfig serialises the merged document like the Python renderer's
// `yaml.safe_dump(sort_keys=False, allow_unicode=True, default_flow_style=False,
// width=4096, indent=2)`.
//
// The ordered tree and every scalar are identical to PyYAML's output. Long
// single-quoted scalars are un-folded after marshaling (go-yaml folds at 80
// columns where PyYAML width=4096 keeps one line), guarded by a re-parse. Other
// emitter choices, such as quoting a numeric-looking string with double quotes,
// are left to go-yaml; parsing either form yields the same document, which is
// what the golden test asserts.
func DumpConfig(data interface{}) (string, error) {
	output, err := yaml.Marshal(data)
	if err != nil {
		return "", err
	}
	folded := string(output)
	unfolded := unfoldSingleQuoted(folded)
	if unfolded == folded {
		return folded, nil
	}
	// Never risk a malformed document: only use the un-folded text when it
	// parses back to exactly the same tree.
	var originalTree, unfoldedTree interface{}
	if err := yaml.Unmarshal([]byte(folded), &originalTree); err != nil {
		return folded, nil
	}
	if err := yaml.Unmarshal([]byte(unfolded), &unfoldedTree); err != nil {
		return folded, nil
	}
	if !reflect.DeepEqual(normalize(originalTree), normalize(unfoldedTree)) {
		return folded, nil
	}
	return unfolded, nil
}

// normalize converts the generic decode result (maps) to a comparable tree.
func normalize(value interface{}) interface{} {
	switch typed := value.(type) {
	case map[interface{}]interface{}:
		result := make(map[string]interface{}, len(typed))
		for key, item := range typed {
			result[fmt.Sprint(key)] = normalize(item)
		}
		return result
	case yaml.MapSlice:
		result := make(map[string]interface{}, len(typed))
		for _, item := range typed {
			result[fmt.Sprint(item.Key)] = normalize(item.Value)
		}
		return result
	case []interface{}:
		result := make([]interface{}, len(typed))
		for index, item := range typed {
			result[index] = normalize(item)
		}
		return result
	default:
		return value
	}
}

// unfoldSingleQuoted joins continuation lines of a single-quoted scalar, which
// go-yaml folds at 80 columns and PyYAML (width=4096) keeps on one line. YAML
// folding turns one line break inside a quoted scalar into one space, so the
// join is lossless for a scalar that was emitted from a one-line value.
func unfoldSingleQuoted(text string) string {
	lines := strings.Split(text, "\n")
	var result []string
	changed := false
	for index := 0; index < len(lines); index++ {
		line := lines[index]
		if strings.Count(line, "'")%2 == 0 {
			result = append(result, line)
			continue
		}
		indent := len(line) - len(strings.TrimLeft(line, " "))
		joined := line
		next := index + 1
		complete := false
		for next < len(lines) {
			candidate := lines[next]
			if strings.TrimSpace(candidate) == "" {
				break
			}
			candidateIndent := len(candidate) - len(strings.TrimLeft(candidate, " "))
			if candidateIndent <= indent {
				break
			}
			joined += " " + strings.TrimLeft(candidate, " ")
			if strings.Count(joined, "'")%2 == 0 {
				complete = true
				next++
				break
			}
			next++
		}
		if !complete {
			result = append(result, line)
			continue
		}
		result = append(result, joined)
		index = next - 1
		changed = true
	}
	if !changed {
		return text
	}
	return strings.Join(result, "\n")
}

// Redact masks token-shaped values so a dry-run diff is safe to paste into chat.
func Redact(text string) string {
	return secretLinePattern.ReplaceAllString(text, "${1}<redacted>")
}

func readMapping(path, what string) (yaml.MapSlice, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	if len(bytes.TrimSpace(raw)) == 0 {
		return yaml.MapSlice{}, nil
	}
	// Decoding into MapSlice makes go-yaml preserve key order recursively
	// (nested mappings and mappings inside sequences are MapSlice too).
	var mapping yaml.MapSlice
	if err := yaml.Unmarshal(raw, &mapping); err != nil {
		var generic interface{}
		if genericErr := yaml.Unmarshal(raw, &generic); genericErr == nil && generic != nil {
			return nil, fmt.Errorf("%s: %s must be a YAML mapping, got %T", path, what, generic)
		}
		return nil, fmt.Errorf("%s: %w", path, err)
	}
	return mapping, nil
}

// BuildPlan returns every file a render would change, in the Python renderer's
// order: config.yaml, the installed context files, then the skills.
func BuildPlan(options Options) ([]File, error) {
	configPath := filepath.Join(options.Home, "config.yaml")
	current := yaml.MapSlice{}
	if _, err := os.Stat(configPath); err == nil {
		read, err := readMapping(configPath, "config.yaml")
		if err != nil {
			return nil, err
		}
		current = read
	}

	fragment, err := readMapping(options.Fragment, "fragment")
	if err != nil {
		return nil, err
	}
	merged := DeepMerge(current, fragment).(yaml.MapSlice)

	ownerChatID := ""
	if options.PrivateValues != "" {
		if _, err := os.Stat(options.PrivateValues); err != nil {
			return nil, fmt.Errorf("%s: private values file not found", options.PrivateValues)
		}
		private, err := readMapping(options.PrivateValues, "private values")
		if err != nil {
			return nil, err
		}
		var found string
		private, found = popString(private, "telegram_owner_chat_id")
		ownerChatID = strings.TrimSpace(found)
		merged = DeepMerge(merged, private).(yaml.MapSlice)
	}
	for _, key := range privateOnlyKeys {
		merged, _ = popKey(merged, key)
	}

	voice, err := os.ReadFile(options.VoicePrompt)
	if err != nil {
		return nil, fmt.Errorf("%s: voice prompt not found", options.VoicePrompt)
	}
	merged, err = setVoicePersonality(merged, strings.TrimSpace(string(voice)))
	if err != nil {
		return nil, err
	}

	if ownerChatID != "" {
		prompt, err := os.ReadFile(options.TelegramPrompt)
		if err != nil {
			return nil, fmt.Errorf("%s: Telegram prompt not found", options.TelegramPrompt)
		}
		merged, err = setTelegramChannelPrompt(merged, ownerChatID, strings.TrimSpace(string(prompt)))
		if err != nil {
			return nil, err
		}
		merged, err = setTelegramHomeChannel(merged, ownerChatID)
		if err != nil {
			return nil, err
		}
	}

	if !options.AllowPlaceholders {
		if victims := PlaceholderViolations(merged, current); len(victims) > 0 {
			var message strings.Builder
			message.WriteString("refusing to render: placeholder values would replace live ones:\n")
			for _, key := range victims {
				message.WriteString("  " + key + "\n")
			}
			message.WriteString("\nPass --private-values <file> with the real endpoints " +
				"(copy hermes-private.example.yaml), or --allow-placeholders " +
				"when rendering a throw-away test home.")
			return nil, fmt.Errorf("%s", message.String())
		}
	}

	var plan []File
	rendered, err := DumpConfig(merged)
	if err != nil {
		return nil, err
	}
	if existing, err := os.ReadFile(configPath); err != nil || string(existing) != rendered {
		plan = append(plan, File{Path: configPath, Content: rendered})
	}
	for _, entry := range installed {
		source := entry.source(options)
		raw, err := os.ReadFile(source)
		if err != nil {
			continue
		}
		destination := filepath.Join(options.Home, entry.name)
		if existing, err := os.ReadFile(destination); err == nil && string(existing) == string(raw) {
			continue
		}
		plan = append(plan, File{Path: destination, Content: string(raw)})
	}
	skills, err := skillPlan(options.Home, options.Skills)
	if err != nil {
		return nil, err
	}
	plan = append(plan, skills...)
	return plan, nil
}

// setVoicePersonality wires voice_prompt.txt into agent.personalities.voice.
func setVoicePersonality(merged yaml.MapSlice, voice string) (yaml.MapSlice, error) {
	agent, err := childMapping(merged, "agent", "config.yaml: agent must be a mapping")
	if err != nil {
		return nil, err
	}
	personalities, err := childMapping(agent, "personalities", "config.yaml: agent.personalities must be a mapping")
	if err != nil {
		return nil, err
	}
	personalities = setKey(personalities, "voice", voice)
	agent = setKey(agent, "personalities", personalities)
	return setKey(merged, "agent", agent), nil
}

func setTelegramChannelPrompt(merged yaml.MapSlice, chatID, prompt string) (yaml.MapSlice, error) {
	platforms, err := childMapping(merged, "platforms", "config.yaml: platforms must be a mapping")
	if err != nil {
		return nil, err
	}
	telegram, err := childMapping(platforms, "telegram", "config.yaml: platforms.telegram must be a mapping")
	if err != nil {
		return nil, err
	}
	overrides, err := childMapping(telegram, "channel_overrides", "config.yaml: platforms.telegram.channel_overrides must be a mapping")
	if err != nil {
		return nil, err
	}
	channel, err := childMapping(overrides, chatID, "config.yaml: platforms.telegram.channel_overrides entry must be a mapping")
	if err != nil {
		return nil, err
	}
	channel = setKey(channel, "system_prompt", prompt)
	overrides = setKey(overrides, chatID, channel)
	telegram = setKey(telegram, "channel_overrides", overrides)
	platforms = setKey(platforms, "telegram", telegram)
	return setKey(merged, "platforms", platforms), nil
}

func setTelegramHomeChannel(merged yaml.MapSlice, chatID string) (yaml.MapSlice, error) {
	platforms, err := childMapping(merged, "platforms", "config.yaml: platforms must be a mapping")
	if err != nil {
		return nil, err
	}
	telegram, err := childMapping(platforms, "telegram", "config.yaml: platforms.telegram must be a mapping")
	if err != nil {
		return nil, err
	}
	name := "Home"
	if current, ok := valueOf(telegram, "home_channel"); ok {
		if mapping, ok := current.(yaml.MapSlice); ok {
			if existing := strings.TrimSpace(stringValue(mapping, "name")); existing != "" {
				name = existing
			}
		}
	}
	telegram = setKey(telegram, "home_channel", yaml.MapSlice{
		{Key: "platform", Value: "telegram"},
		{Key: "chat_id", Value: chatID},
		{Key: "name", Value: name},
	})
	platforms = setKey(platforms, "telegram", telegram)
	return setKey(merged, "platforms", platforms), nil
}

func skillPlan(home, skillsDir string) ([]File, error) {
	info, err := os.Stat(skillsDir)
	if err != nil || !info.IsDir() {
		return nil, nil
	}
	entries, err := os.ReadDir(skillsDir)
	if err != nil {
		return nil, err
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i].Name() < entries[j].Name() })
	var plan []File
	for _, entry := range entries {
		skillDir := filepath.Join(skillsDir, entry.Name())
		// os.Stat, not DirEntry/Lstat: a Bazel runfiles tree (and many
		// symlinked volumes) represents files and directories as symlinks.
		info, err := os.Stat(skillDir)
		if err != nil || !info.IsDir() {
			continue
		}
		if stats, err := os.Stat(filepath.Join(skillDir, "SKILL.md")); err != nil || !stats.Mode().IsRegular() {
			continue
		}
		err = filepath.Walk(skillDir, func(path string, _ os.FileInfo, err error) error {
			if err != nil {
				return err
			}
			stats, err := os.Stat(path)
			if err != nil || !stats.Mode().IsRegular() {
				return err
			}
			relative, err := filepath.Rel(skillsDir, path)
			if err != nil {
				return err
			}
			destination := filepath.Join(home, "skills", relative)
			raw, err := os.ReadFile(path)
			if err != nil {
				return err
			}
			if existing, err := os.ReadFile(destination); err == nil && string(existing) == string(raw) {
				return nil
			}
			plan = append(plan, File{Path: destination, Content: string(raw)})
			return nil
		})
		if err != nil {
			return nil, err
		}
	}
	return plan, nil
}

// DiffFor returns the redacted unified diff of one planned write.
func DiffFor(path, newText string) string {
	old := ""
	if raw, err := os.ReadFile(path); err == nil {
		old = string(raw)
	}
	diff := unifiedDiff(path, old, newText)
	return Redact(diff)
}

// ApplyPlan writes the plan, keeping at most one `.prev` copy (mode 600).
func ApplyPlan(plan []File, stdout io.Writer) ([]string, error) {
	written := make([]string, 0, len(plan))
	stamp := time.Now().UTC().Format("2006-01-02T15:04:05Z")
	for _, file := range plan {
		if raw, err := os.ReadFile(file.Path); err == nil {
			backup := file.Path + ".prev"
			if err := os.WriteFile(backup, raw, 0o600); err != nil {
				return written, err
			}
			if err := os.Chmod(backup, 0o600); err != nil {
				return written, err
			}
		}
		if err := os.MkdirAll(filepath.Dir(file.Path), 0o755); err != nil {
			return written, err
		}
		if err := os.WriteFile(file.Path, []byte(file.Content), 0o644); err != nil {
			return written, err
		}
		written = append(written, file.Path)
	}
	fmt.Fprintf(stdout, "rendered %d file(s) at %s\n", len(written), stamp)
	return written, nil
}

// Run renders the overlay. It returns the process exit status: 1 for --check
// with pending changes, otherwise 0.
func Run(options Options, dryRun, check bool, stdout io.Writer) (int, error) {
	info, err := os.Stat(options.Home)
	if err != nil || !info.IsDir() {
		return 0, fmt.Errorf("%s: not a directory (pass the mounted HERMES_HOME)", options.Home)
	}
	if _, err := os.Stat(options.Fragment); err != nil {
		return 0, fmt.Errorf("%s: fragment not found", options.Fragment)
	}
	plan, err := BuildPlan(options)
	if err != nil {
		return 0, err
	}
	if len(plan) == 0 {
		fmt.Fprintln(stdout, "up to date: config.yaml, .hermes.md, SOUL.md and skills already match the repository")
		return 0, nil
	}
	for _, file := range plan {
		fmt.Fprintf(stdout, "--- would update %s ---\n", file.Path)
		fmt.Fprint(stdout, DiffFor(file.Path, file.Content))
	}
	if dryRun || check {
		if check {
			return 1, nil
		}
		return 0, nil
	}
	if _, err := ApplyPlan(plan, stdout); err != nil {
		return 0, err
	}
	fmt.Fprintln(stdout, "restart the gateway to load the new config and context")
	return 0, nil
}

// --- ordered-map helpers -------------------------------------------------

func indexOfKey(mapping yaml.MapSlice, key interface{}) int {
	for index, item := range mapping {
		if item.Key == key || fmt.Sprint(item.Key) == fmt.Sprint(key) {
			return index
		}
	}
	return -1
}

func valueOf(mapping yaml.MapSlice, key string) (interface{}, bool) {
	index := indexOfKey(mapping, key)
	if index < 0 {
		return nil, false
	}
	return mapping[index].Value, true
}

func stringValue(mapping yaml.MapSlice, key string) string {
	value, ok := valueOf(mapping, key)
	if !ok || value == nil {
		return ""
	}
	text, _ := value.(string)
	return text
}

func setKey(mapping yaml.MapSlice, key string, value interface{}) yaml.MapSlice {
	index := indexOfKey(mapping, key)
	if index >= 0 {
		mapping[index].Value = value
		return mapping
	}
	return append(mapping, yaml.MapItem{Key: key, Value: value})
}

func popKey(mapping yaml.MapSlice, key string) (yaml.MapSlice, interface{}) {
	index := indexOfKey(mapping, key)
	if index < 0 {
		return mapping, nil
	}
	value := mapping[index].Value
	return append(mapping[:index:index], mapping[index+1:]...), value
}

func popString(mapping yaml.MapSlice, key string) (yaml.MapSlice, string) {
	updated, value := popKey(mapping, key)
	text, _ := value.(string)
	return updated, text
}

func childMapping(parent yaml.MapSlice, key, message string) (yaml.MapSlice, error) {
	value, ok := valueOf(parent, key)
	if !ok || value == nil {
		return yaml.MapSlice{}, nil
	}
	mapping, ok := value.(yaml.MapSlice)
	if !ok {
		return nil, fmt.Errorf("%s", message)
	}
	return mapping, nil
}
