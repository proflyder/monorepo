package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func writeFile(t *testing.T, path, content string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(path, []byte(content), 0o644); err != nil {
		t.Fatal(err)
	}
}

func fakeLint(t *testing.T) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "lint")
	script := `#!/bin/sh
printf '%s\n' "$@" > "$LINT_ARGS_FILE"
printf 'user_home=%s\n' "$ANDROID_USER_HOME"
printf 'lint_opts=%s\n' "$LINT_OPTS"
exit "${LINT_EXIT_CODE:-0}"
`
	if err := os.WriteFile(path, []byte(script), 0o755); err != nil {
		t.Fatal(err)
	}
	return path
}

type lintFixture struct {
	root        string
	recordsPath string
	configPath  string
	workdir     string
	argsFile    string
	lintPath    string
	sdkHome     string
}

func newLintFixture(t *testing.T) lintFixture {
	t.Helper()
	temp := t.TempDir()
	t.Setenv("TEST_SRCDIR", filepath.Join(temp, "src"))
	t.Setenv("TEST_WORKSPACE", "ws")
	root := filepath.Join(temp, "src", "ws")
	t.Setenv("TEST_TMPDIR", filepath.Join(temp, "work"))
	workdir := filepath.Join(temp, "work")
	argsFile := filepath.Join(temp, "lint-args.txt")
	t.Setenv("LINT_ARGS_FILE", argsFile)
	sdkHome := filepath.Join(temp, "sdk")
	t.Setenv("ANDROID_LINT", "")
	t.Setenv("ANDROID_HOME", sdkHome)
	t.Setenv("LINT_EXIT_CODE", "")

	writeFile(t, filepath.Join(root, "app", "ViewBindings.kt"), "package dev.example.databinding\nclass X\n")
	writeFile(t, filepath.Join(root, "app", "Main.kt"), "package dev.example\n")
	writeFile(t, filepath.Join(root, "app", "AndroidManifest.xml"), "<manifest />\n")
	writeFile(t, filepath.Join(root, "app", "res", "layout", "a.xml"), "<LinearLayout />\n")
	writeFile(t, filepath.Join(root, "libs", "foo.jar"), "jar")

	manifest := "app/AndroidManifest.xml"
	modules := []module{
		{
			Label:         "//app:app",
			Name:          "//app:app",
			Package:       "dev.example",
			Android:       true,
			App:           true,
			Srcs:          []string{"app/ViewBindings.kt", "app/Main.kt"},
			GeneratedSrcs: []string{"app/ViewBindings.kt"},
			Resources:     []string{"app/res/layout/a.xml"},
			Manifest:      &manifest,
			Jars:          []string{"libs/foo.jar", "libs/missing.jar"},
			Deps:          []string{"//lib:lib", "//absent:absent"},
		},
		{
			Label:   "//lib:lib",
			Name:    "//lib:lib",
			Package: "dev.example.lib",
			Android: true,
			App:     false,
		},
	}
	var lines []string
	for _, item := range modules {
		payload, err := json.Marshal(item)
		if err != nil {
			t.Fatal(err)
		}
		lines = append(lines, string(payload))
	}
	recordsPath := filepath.Join(root, "modules.jsonl")
	writeFile(t, recordsPath, strings.Join(lines, "\n")+"\n")
	configPath := filepath.Join(root, "lint.xml")
	writeFile(t, configPath, "<lint />\n")

	return lintFixture{
		root:        root,
		recordsPath: recordsPath,
		configPath:  configPath,
		workdir:     workdir,
		argsFile:    argsFile,
		lintPath:    fakeLint(t),
		sdkHome:     sdkHome,
	}
}

func runLint(t *testing.T, args ...string) (int, string, string) {
	t.Helper()
	var stdout, stderr bytes.Buffer
	code := run(args, &stdout, &stderr)
	return code, stdout.String(), stderr.String()
}

func TestMainFlowBuildsProjectAndRunsLint(t *testing.T) {
	fixture := newLintFixture(t)
	t.Setenv("ANDROID_LINT", fixture.lintPath)

	code, stdout, stderr := runLint(t, fixture.recordsPath, fixture.configPath)
	if code != 0 {
		t.Fatalf("exit %d: %s%s", code, stdout, stderr)
	}

	projectPath := filepath.Join(fixture.workdir, "project.xml")
	project, err := os.ReadFile(projectPath)
	if err != nil {
		t.Fatal(err)
	}
	text := string(project)
	for _, want := range []string{
		`<project android="true" incomplete="false">`,
		`<root dir="` + fixture.root + `" />`,
		`<module name="//app:app" android="true" compile-sdk-version="36" javaLanguage="17" kotlinLanguage="2.4" library="false">`,
		`<module name="//lib:lib" android="true" compile-sdk-version="36" javaLanguage="17" kotlinLanguage="2.4" library="true" />`,
		`<resource file="` + filepath.Join(fixture.root, "app", "res", "layout", "a.xml") + `" />`,
		`<manifest file="` + filepath.Join(fixture.root, "app", "AndroidManifest.xml") + `" />`,
		`<classpath jar="` + filepath.Join(fixture.root, "libs", "foo.jar") + `" />`,
		`<dep module="//lib:lib" />`,
	} {
		if !strings.Contains(text, want) {
			t.Errorf("project.xml does not contain %q:\n%s", want, text)
		}
	}
	if strings.Contains(text, "missing.jar") {
		t.Error("missing jar was added to the classpath")
	}
	if strings.Contains(text, "//absent:absent") {
		t.Error("unknown dependency was added to the module")
	}
	staged := filepath.Join(fixture.workdir, "generated", "__app_app", "dev", "example", "databinding", "ViewBindings.kt")
	if !strings.Contains(text, `<src file="`+staged+`" generated="true" />`) {
		t.Errorf("generated source not staged at %s:\n%s", staged, text)
	}
	if _, err := os.Stat(staged); err != nil {
		t.Errorf("staged generated source missing: %v", err)
	}

	mergedPath := filepath.Join(fixture.workdir, "__app_app-merged-manifest.xml")
	merged, err := os.ReadFile(mergedPath)
	if err != nil {
		t.Fatal(err)
	}
	for _, want := range []string{
		`package="dev.example"`,
		`xmlns:ns0="http://schemas.android.com/apk/res/android"`,
		`ns0:versionCode="1"`,
		`ns0:versionName="0.1.0"`,
		`ns0:minSdkVersion="26"`,
		`ns0:targetSdkVersion="36"`,
	} {
		if !strings.Contains(string(merged), want) {
			t.Errorf("merged manifest does not contain %q:\n%s", want, merged)
		}
	}

	args, err := os.ReadFile(fixture.argsFile)
	if err != nil {
		t.Fatal(err)
	}
	argText := string(args)
	for _, want := range []string{
		"--project\n" + projectPath,
		"--compile-sdk-version\n36",
		"--sdk-home\n" + fixture.sdkHome,
		"--config\n" + fixture.configPath,
		"--exitcode\n-Werror",
		"--text\nstdout",
		"--quiet",
	} {
		if !strings.Contains(argText, want) {
			t.Errorf("lint args do not contain %q:\n%s", want, argText)
		}
	}
	if !strings.Contains(stdout, "user_home="+filepath.Join(fixture.workdir, "android-user")) {
		t.Errorf("ANDROID_USER_HOME not set: %s", stdout)
	}
	if !strings.Contains(stdout, "lint_opts=-Duser.home="+fixture.workdir) {
		t.Errorf("LINT_OPTS not set: %s", stdout)
	}
}

func TestExitCodeIsPropagated(t *testing.T) {
	fixture := newLintFixture(t)
	t.Setenv("ANDROID_LINT", fixture.lintPath)
	t.Setenv("LINT_EXIT_CODE", "3")

	code, _, _ := runLint(t, fixture.recordsPath, fixture.configPath)
	if code != 3 {
		t.Fatalf("exit %d, want 3", code)
	}
}

func TestLintModulesEnvironmentFallback(t *testing.T) {
	fixture := newLintFixture(t)
	t.Setenv("ANDROID_LINT", fixture.lintPath)
	t.Setenv("LINT_MODULES", fixture.recordsPath)

	code, _, stderr := runLint(t)
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}
}

func TestMissingRunfilesFails(t *testing.T) {
	t.Setenv("TEST_SRCDIR", "")
	t.Setenv("TEST_WORKSPACE", "")
	code, _, stderr := runLint(t, "modules.jsonl")
	if code != 1 || !strings.Contains(stderr, "TEST_SRCDIR/TEST_WORKSPACE are not set") {
		t.Fatalf("exit %d: %s", code, stderr)
	}
}

func TestMissingArgumentsFails(t *testing.T) {
	t.Setenv("TEST_SRCDIR", t.TempDir())
	t.Setenv("TEST_WORKSPACE", "ws")
	t.Setenv("LINT_MODULES", "")
	code, _, stderr := runLint(t)
	if code != 1 || !strings.Contains(stderr, "usage: lint_runner") {
		t.Fatalf("exit %d: %s", code, stderr)
	}
}

func TestFindLintUsesAndroidHome(t *testing.T) {
	temp := t.TempDir()
	lintPath := filepath.Join(temp, "cmdline-tools", "23.0", "bin", "lint")
	if err := os.MkdirAll(filepath.Dir(lintPath), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(lintPath, []byte("#!/bin/sh\n"), 0o755); err != nil {
		t.Fatal(err)
	}
	t.Setenv("ANDROID_LINT", "")
	t.Setenv("ANDROID_HOME", temp)
	t.Setenv("ANDROID_SDK_ROOT", "")

	found, err := findLint()
	if err != nil {
		t.Fatal(err)
	}
	if found != lintPath {
		t.Fatalf("found %s, want %s", found, lintPath)
	}
}

func TestFindLintFailsWithoutSdk(t *testing.T) {
	t.Setenv("ANDROID_LINT", "")
	t.Setenv("ANDROID_HOME", "")
	t.Setenv("ANDROID_SDK_ROOT", "")
	if _, err := findLint(); err == nil || !strings.Contains(err.Error(), "Android Lint was not found") {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestStagedGeneratedSourceWithoutPackageKeepsPath(t *testing.T) {
	temp := t.TempDir()
	path := filepath.Join(temp, "NoPackage.kt")
	writeFile(t, path, "class X\n")
	staged, err := stagedGeneratedSource(module{Name: "//m:m"}, path, temp)
	if err != nil {
		t.Fatal(err)
	}
	if staged != path {
		t.Fatalf("staged %s, want original %s", staged, path)
	}
}
