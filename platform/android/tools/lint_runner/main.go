// Command lint_runner runs Android Lint over the Bazel modules of the Home
// Android client.
//
// The Bazel rule (`android_lint` in lint.bzl) writes one JSON record per module
// with the runfiles-relative paths of its sources, resources and manifest. This
// tool turns those records into the `project.xml` layout descriptor understood
// by the standalone Android Lint CLI and runs lint once over the whole project,
// so cross-module resource usage is analysed as a single build rather than as
// unrelated modules.
//
// The lint tool is not a Bazel input: it ships with the Android SDK
// `cmdline-tools` package. The Bazel rule sets ANDROID_HOME/ANDROID_SDK_ROOT in
// the test environment; CI installs the pinned `cmdline-tools;23.0`.
package main

import (
	"bufio"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strings"
	"unicode"
)

const (
	minSDK      = "26"
	targetSDK   = "36"
	compileSDK  = "36"
	versionCode = "1"
	versionName = "0.1.0"

	androidNS = "http://schemas.android.com/apk/res/android"
)

var packagePattern = regexp.MustCompile(`(?m)^package\s+([\w.]+)`)

type module struct {
	Label         string   `json:"label"`
	Name          string   `json:"name"`
	Package       string   `json:"package"`
	Android       bool     `json:"android"`
	App           bool     `json:"app"`
	Srcs          []string `json:"srcs"`
	GeneratedSrcs []string `json:"generated_srcs"`
	Resources     []string `json:"resources"`
	Manifest      *string  `json:"manifest"`
	Deps          []string `json:"deps"`
	Jars          []string `json:"jars"`
}

type attribute struct {
	name  string
	value string
}

type element struct {
	tag      string
	attrs    []attribute
	children []*element
}

func newElement(tag string) *element {
	return &element{tag: tag}
}

func (e *element) attr(name, value string) {
	e.attrs = append(e.attrs, attribute{name: name, value: value})
}

func (e *element) child(tag string) *element {
	child := newElement(tag)
	e.children = append(e.children, child)
	return child
}

func escapeAttr(value string) string {
	var b strings.Builder
	for _, r := range value {
		switch r {
		case '&':
			b.WriteString("&amp;")
		case '<':
			b.WriteString("&lt;")
		case '>':
			b.WriteString("&gt;")
		case '"':
			b.WriteString("&quot;")
		case '\r':
			b.WriteString("&#13;")
		case '\n':
			b.WriteString("&#10;")
		case '\t':
			b.WriteString("&#09;")
		default:
			b.WriteRune(r)
		}
	}
	return b.String()
}

// splitQName separates ElementTree-style `{uri}local` names.
func splitQName(name string) (string, string) {
	if strings.HasPrefix(name, "{") {
		if end := strings.Index(name, "}"); end >= 0 {
			return name[1:end], name[end+1:]
		}
	}
	return "", name
}

// collectNamespaces assigns ns0, ns1, ... to namespaced attributes in document
// order, mirroring ElementTree's serialization of the lint model.
func collectNamespaces(e *element, prefixes map[string]string, order *[]string) {
	for _, attr := range e.attrs {
		uri, _ := splitQName(attr.name)
		if uri == "" {
			continue
		}
		if _, exists := prefixes[uri]; !exists {
			prefixes[uri] = fmt.Sprintf("ns%d", len(*order))
			*order = append(*order, uri)
		}
	}
	for _, child := range e.children {
		collectNamespaces(child, prefixes, order)
	}
}

func (e *element) write(b *strings.Builder, prefixes map[string]string, order []string, isRoot bool) {
	tagURI, tagLocal := splitQName(e.tag)
	tag := tagLocal
	if tagURI != "" {
		tag = prefixes[tagURI] + ":" + tagLocal
	}
	b.WriteString("<")
	b.WriteString(tag)
	if isRoot {
		for index, uri := range order {
			b.WriteString(" ")
			b.WriteString(fmt.Sprintf("xmlns:ns%d", index))
			b.WriteString(`="`)
			b.WriteString(escapeAttr(uri))
			b.WriteString(`"`)
		}
	}
	for _, attr := range e.attrs {
		name := attr.name
		if uri, local := splitQName(attr.name); uri != "" {
			name = prefixes[uri] + ":" + local
		}
		b.WriteString(" ")
		b.WriteString(name)
		b.WriteString(`="`)
		b.WriteString(escapeAttr(attr.value))
		b.WriteString(`"`)
	}
	if len(e.children) == 0 {
		b.WriteString(" />")
		return
	}
	b.WriteString(">")
	for _, child := range e.children {
		child.write(b, prefixes, order, false)
	}
	b.WriteString("</")
	b.WriteString(tag)
	b.WriteString(">")
}

func writeDocument(path string, root *element) error {
	prefixes := map[string]string{}
	var order []string
	collectNamespaces(root, prefixes, &order)

	var b strings.Builder
	b.WriteString("<?xml version='1.0' encoding='utf-8'?>\n")
	root.write(&b, prefixes, order, true)
	return os.WriteFile(path, []byte(b.String()), 0o644)
}

// joinPath mirrors os.path.join: it concatenates without cleaning `..`.
// The lint records contain runfiles-relative paths such as
// `../rules_jvm_external++maven+maven/...`; normalising them would produce a
// different (but equivalent) project model.
func joinPath(base, name string) string {
	if filepath.IsAbs(name) {
		return name
	}
	if base == "" {
		return name
	}
	if strings.HasSuffix(base, string(filepath.Separator)) {
		return base + name
	}
	return base + string(filepath.Separator) + name
}

func runfilesRoot() (string, error) {
	testSrcDir := os.Getenv("TEST_SRCDIR")
	testWorkspace := os.Getenv("TEST_WORKSPACE")
	if testSrcDir == "" || testWorkspace == "" {
		return "", fmt.Errorf("TEST_SRCDIR/TEST_WORKSPACE are not set; run through bazel test")
	}
	return joinPath(testSrcDir, testWorkspace), nil
}

func isExecutableFile(path string) bool {
	info, err := os.Stat(path)
	if err != nil || info.IsDir() {
		return false
	}
	return info.Mode().Perm()&0o111 != 0
}

func findLint() (string, error) {
	if override := os.Getenv("ANDROID_LINT"); override != "" {
		return override, nil
	}
	homes := []string{os.Getenv("ANDROID_HOME"), os.Getenv("ANDROID_SDK_ROOT")}
	var candidates []string
	for _, home := range homes {
		if home == "" {
			continue
		}
		candidates = append(candidates,
			filepath.Join(home, "cmdline-tools", "23.0", "bin", "lint"),
			filepath.Join(home, "cmdline-tools", "latest", "bin", "lint"),
		)
		cmdline := filepath.Join(home, "cmdline-tools")
		if entries, err := os.ReadDir(cmdline); err == nil {
			var names []string
			for _, entry := range entries {
				names = append(names, entry.Name())
			}
			sort.Sort(sort.Reverse(sort.StringSlice(names)))
			for _, name := range names {
				candidates = append(candidates, filepath.Join(cmdline, name, "bin", "lint"))
			}
		}
		candidates = append(candidates, filepath.Join(home, "tools", "bin", "lint"))
	}
	for _, candidate := range candidates {
		if isExecutableFile(candidate) {
			return candidate, nil
		}
	}
	return "", fmt.Errorf("Android Lint was not found. Install the pinned SDK package " +
		"`cmdline-tools;23.0` (sdkmanager --install \"cmdline-tools;23.0\") " +
		"or set ANDROID_LINT to the lint executable.")
}

func safeName(name string) string {
	var b strings.Builder
	for _, r := range name {
		if unicode.IsLetter(r) || unicode.IsDigit(r) || r == '.' || r == '_' || r == '-' {
			b.WriteRune(r)
		} else {
			b.WriteRune('_')
		}
	}
	return b.String()
}

// stagedGeneratedSource copies a generated source into a directory matching its
// package. Lint warns when a generated file's path does not match its package
// (the ViewBinding generator emits files into the Bazel package, not the Kotlin
// package tree). Staging a copy keeps the analysis output clean without
// changing build outputs.
func stagedGeneratedSource(item module, path, workdir string) (string, error) {
	payload, err := os.ReadFile(path)
	if err != nil {
		return "", err
	}
	match := packagePattern.FindSubmatch(payload)
	if match == nil {
		return path, nil
	}
	packageName := string(match[1])
	parts := append([]string{workdir, "generated", safeName(item.Name)}, strings.Split(packageName, ".")...)
	destDir := filepath.Join(parts...)
	if err := os.MkdirAll(destDir, 0o755); err != nil {
		return "", err
	}
	dest := filepath.Join(destDir, filepath.Base(path))
	if err := os.WriteFile(dest, payload, 0o644); err != nil {
		return "", err
	}
	return dest, nil
}

// mergedManifest writes a merged manifest carrying the SDK levels Bazel
// configures. Library source manifests are intentionally empty; the application
// manifest declares minSdk/targetSdk. Lint reads SDK levels from the merged
// manifest, so every module gets one. The application module also needs the
// version fields that android_binary injects through manifest_values.
func mergedManifest(item module, workdir string) (string, error) {
	manifest := newElement("manifest")
	manifest.attr("package", item.Package)
	usesSDK := manifest.child("uses-sdk")
	usesSDK.attr("{"+androidNS+"}minSdkVersion", minSDK)
	usesSDK.attr("{"+androidNS+"}targetSdkVersion", targetSDK)
	if item.App {
		manifest.attr("{"+androidNS+"}versionCode", versionCode)
		manifest.attr("{"+androidNS+"}versionName", versionName)
	}

	path := filepath.Join(workdir, safeName(item.Name)+"-merged-manifest.xml")
	if err := writeDocument(path, manifest); err != nil {
		return "", err
	}
	return path, nil
}

func readModules(path string) ([]module, error) {
	file, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer file.Close()

	var modules []module
	scanner := bufio.NewScanner(file)
	scanner.Buffer(make([]byte, 64*1024), 64*1024*1024)
	for scanner.Scan() {
		line := scanner.Text()
		if strings.TrimSpace(line) == "" {
			continue
		}
		var item module
		if err := json.Unmarshal([]byte(line), &item); err != nil {
			return nil, err
		}
		modules = append(modules, item)
	}
	if err := scanner.Err(); err != nil {
		return nil, err
	}
	return modules, nil
}

func buildProject(modules []module, root, workdir string) (*element, error) {
	names := map[string]bool{}
	for _, item := range modules {
		names[item.Name] = true
	}

	project := newElement("project")
	project.attr("android", "true")
	project.attr("incomplete", "false")
	project.child("root").attr("dir", root)

	for _, item := range modules {
		attrs := []attribute{
			{"name", item.Name},
			{"android", boolString(item.Android)},
			{"compile-sdk-version", compileSDK},
			{"javaLanguage", "17"},
			{"kotlinLanguage", "2.4"},
		}
		if item.Android {
			attrs = append(attrs, attribute{"library", boolString(!item.App)})
		}
		moduleElement := newElement("module")
		moduleElement.attrs = attrs

		generated := map[string]bool{}
		for _, src := range item.GeneratedSrcs {
			generated[src] = true
		}
		for _, src := range item.Srcs {
			path := joinPath(root, src)
			srcElement := newElement("src")
			srcElement.attr("file", path)
			if generated[src] {
				srcElement.attr("generated", "true")
				staged, err := stagedGeneratedSource(item, path, workdir)
				if err != nil {
					return nil, err
				}
				srcElement.attrs[0].value = staged
			}
			moduleElement.children = append(moduleElement.children, srcElement)
		}
		for _, resource := range item.Resources {
			moduleElement.child("resource").attr("file", joinPath(root, resource))
		}
		if item.Manifest != nil {
			moduleElement.child("manifest").attr("file", joinPath(root, *item.Manifest))
			merged, err := mergedManifest(item, workdir)
			if err != nil {
				return nil, err
			}
			moduleElement.child("merged-manifest").attr("file", merged)
		}
		for _, jar := range item.Jars {
			jarPath := joinPath(root, jar)
			if _, err := os.Stat(jarPath); err == nil {
				moduleElement.child("classpath").attr("jar", jarPath)
			}
		}
		for _, dep := range item.Deps {
			if names[dep] {
				moduleElement.child("dep").attr("module", dep)
			}
		}
		project.children = append(project.children, moduleElement)
	}
	return project, nil
}

func boolString(value bool) string {
	if value {
		return "true"
	}
	return "false"
}

func setDefaultEnv(env map[string]string, order *[]string, key, value string) {
	if _, exists := env[key]; exists {
		return
	}
	env[key] = value
	*order = append(*order, key)
}

func run(args []string, stdout, stderr io.Writer) int {
	flags := flag.NewFlagSet("lint_runner", flag.ContinueOnError)
	flags.SetOutput(stderr)
	if err := flags.Parse(args); err != nil {
		return 2
	}
	positional := flags.Args()

	recordsPath := ""
	if len(positional) >= 1 {
		recordsPath = positional[0]
	} else if value := os.Getenv("LINT_MODULES"); value != "" {
		recordsPath = value
	} else {
		fmt.Fprintln(stderr, "usage: lint_runner <modules.jsonl> [lint.xml] (or set LINT_MODULES)")
		return 1
	}
	configPath := ""
	if len(positional) >= 2 {
		configPath = positional[1]
	}

	root, err := runfilesRoot()
	if err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}
	if !filepath.IsAbs(recordsPath) {
		recordsPath = joinPath(root, recordsPath)
	}

	modules, err := readModules(recordsPath)
	if err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}

	tmp := os.Getenv("TEST_TMPDIR")
	if tmp == "" {
		tmp, err = os.MkdirTemp("", "lint-")
		if err != nil {
			fmt.Fprintln(stderr, err)
			return 1
		}
	}
	if err := os.MkdirAll(tmp, 0o755); err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}

	project, err := buildProject(modules, root, tmp)
	if err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}
	projectPath := filepath.Join(tmp, "project.xml")
	if err := writeDocument(projectPath, project); err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}

	if configPath == "" {
		configPath = joinPath(root, "projects/mobile/android/lint.xml")
	}
	lint, err := findLint()
	if err != nil {
		fmt.Fprintln(stderr, err)
		return 1
	}

	sdkHome := os.Getenv("ANDROID_HOME")
	if sdkHome == "" {
		sdkHome = os.Getenv("ANDROID_SDK_ROOT")
	}
	command := []string{
		lint,
		"--project", projectPath,
		"--compile-sdk-version", compileSDK,
	}
	if sdkHome != "" {
		command = append(command, "--sdk-home", sdkHome)
	}
	command = append(command,
		"--config", configPath,
		"--exitcode", "-Werror",
		"--text", "stdout",
		"--quiet",
	)

	// Rebuild the environment in a deterministic order: inherited entries keep
	// their order, then the two defaults (only when absent).
	envOrder := os.Environ()
	env := make(map[string]string, len(envOrder)+2)
	for _, entry := range envOrder {
		key := entry
		value := ""
		if index := strings.IndexByte(entry, '='); index >= 0 {
			key = entry[:index]
			value = entry[index+1:]
		}
		env[key] = value
	}
	extraOrder := []string{}
	setDefaultEnv(env, &extraOrder, "ANDROID_USER_HOME", filepath.Join(tmp, "android-user"))
	setDefaultEnv(env, &extraOrder, "LINT_OPTS", "-Duser.home="+tmp)

	var commandEnv []string
	seen := map[string]bool{}
	for _, entry := range envOrder {
		key := entry
		if index := strings.IndexByte(entry, '='); index >= 0 {
			key = entry[:index]
		}
		if seen[key] {
			continue
		}
		seen[key] = true
		commandEnv = append(commandEnv, key+"="+env[key])
	}
	for _, key := range extraOrder {
		commandEnv = append(commandEnv, key+"="+env[key])
	}

	cmd := exec.Command(command[0], command[1:]...)
	cmd.Stdout = stdout
	cmd.Stderr = stderr
	cmd.Stdin = os.Stdin
	cmd.Env = commandEnv
	err = cmd.Run()
	if err != nil {
		if exitError, ok := err.(*exec.ExitError); ok {
			return exitError.ExitCode()
		}
		fmt.Fprintln(stderr, err)
		return 1
	}
	return 0
}

func main() {
	os.Exit(run(os.Args[1:], os.Stdout, os.Stderr))
}
