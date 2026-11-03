package main

import (
	"bytes"
	"encoding/json"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

var configFixture = map[string]any{
	"project_info": map[string]any{
		"project_number": "1000000000000",
		"project_id":     "example-project",
		"storage_bucket": "example-project.firebasestorage.app",
	},
	"client": []any{
		map[string]any{
			"client_info": map[string]any{
				"mobilesdk_app_id":    "1:1000000000000:android:0123456789abcdef",
				"android_client_info": map[string]any{"package_name": "dev.example.other"},
			},
			"api_key":      []any{map[string]any{"current_key": "ExampleKeyOtherClient"}},
			"oauth_client": []any{},
		},
		map[string]any{
			"client_info": map[string]any{
				"mobilesdk_app_id":    "1:1000000000000:android:fedcba9876543210",
				"android_client_info": map[string]any{"package_name": "dev.example.home"},
			},
			"api_key":      []any{map[string]any{"current_key": "ExampleKeyHomeClient"}},
			"oauth_client": []any{},
		},
	},
	"configuration_version": "1",
}

func runGenerator(t *testing.T, config map[string]any, packageName, versionName, versionCode string) (int, string, string) {
	t.Helper()
	temp := t.TempDir()
	args := []string{
		"--package", packageName,
		"--version-name", versionName,
		"--version-code", versionCode,
		"--output", filepath.Join(temp, "res", "values", "google_services.xml"),
	}
	if config != nil {
		input := filepath.Join(temp, "google-services.json")
		payload, err := json.Marshal(config)
		if err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(input, payload, 0o644); err != nil {
			t.Fatal(err)
		}
		args = append(args, "--input", input)
	}
	var stdout, stderr bytes.Buffer
	code := run(args, &stdout, &stderr)
	output := filepath.Join(temp, "res", "values", "google_services.xml")
	xml, err := os.ReadFile(output)
	if err != nil {
		return code, "", stderr.String()
	}
	return code, string(xml), stderr.String()
}

func stringValue(t *testing.T, xml, name string) string {
	t.Helper()
	pattern := regexp.MustCompile(`<string name="` + regexp.QuoteMeta(name) + `" translatable="false">([^<]*)</string>`)
	match := pattern.FindStringSubmatch(xml)
	if match == nil {
		return ""
	}
	return match[1]
}

func TestSelectsRequestedPackageAndEmitsGradleEquivalentStrings(t *testing.T) {
	code, xml, stderr := runGenerator(t, configFixture, "dev.example.home", "0.1.0", "1")
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}
	want := map[string]string{
		"google_app_id":                  "1:1000000000000:android:fedcba9876543210",
		"gcm_defaultSenderId":            "1000000000000",
		"google_api_key":                 "ExampleKeyHomeClient",
		"google_crash_reporting_api_key": "ExampleKeyHomeClient",
		"google_storage_bucket":          "example-project.firebasestorage.app",
		"project_id":                     "example-project",
	}
	for name, expected := range want {
		if got := stringValue(t, xml, name); got != expected {
			t.Errorf("%s = %q, want %q", name, got, expected)
		}
	}
	if strings.Contains(xml, "ExampleKeyOtherClient") {
		t.Errorf("other client leaked into output: %s", xml)
	}
}

func TestEmitsCrashlyticsBuildID(t *testing.T) {
	code, xml, stderr := runGenerator(t, configFixture, "dev.example.home", "0.1.0", "1")
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}
	id := stringValue(t, xml, "com.google.firebase.crashlytics.mapping_file_id")
	if !regexp.MustCompile(`^[0-9a-f]{32}$`).MatchString(id) {
		t.Errorf("build id %q is not 32 lowercase hex chars", id)
	}
}

func TestBuildIDIsDeterministicAndChangesWithVersion(t *testing.T) {
	_, first, _ := runGenerator(t, configFixture, "dev.example.home", "0.1.0", "1")
	_, second, _ := runGenerator(t, configFixture, "dev.example.home", "0.1.0", "1")
	_, third, _ := runGenerator(t, configFixture, "dev.example.home", "0.1.0", "2")
	name := "com.google.firebase.crashlytics.mapping_file_id"
	if stringValue(t, first, name) != stringValue(t, second, name) {
		t.Error("build id is not deterministic")
	}
	if stringValue(t, first, name) == stringValue(t, third, name) {
		t.Error("build id did not change with version code")
	}
}

func TestMissingPackageIsAnError(t *testing.T) {
	code, xml, stderr := runGenerator(t, configFixture, "dev.example.absent", "0.1.0", "1")
	if code == 0 {
		t.Fatal("missing package accepted")
	}
	if !strings.Contains(stderr, "no client for package") {
		t.Errorf("unexpected stderr: %s", stderr)
	}
	if xml != "" {
		t.Errorf("output written on error: %s", xml)
	}
}

func TestAbsentConfigWarnsAndEmitsNoFirebaseStrings(t *testing.T) {
	code, xml, stderr := runGenerator(t, nil, "dev.example.home", "0.1.0", "1")
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr)
	}
	if !strings.Contains(stderr, "WARNING") {
		t.Errorf("missing warning: %s", stderr)
	}
	for _, name := range append(append([]string{}, firebaseResources...), crashlyticsBuildIDResource) {
		if strings.Contains(xml, name) {
			t.Errorf("empty config emitted %s: %s", name, xml)
		}
	}
	if !strings.Contains(xml, "<resources>") {
		t.Errorf("not a values file: %s", xml)
	}
}

func TestAbsentFileWithExplicitInputWarns(t *testing.T) {
	temp := t.TempDir()
	output := filepath.Join(temp, "google_services.xml")
	var stdout, stderr bytes.Buffer
	code := run([]string{
		"--package", "dev.example.home",
		"--input", filepath.Join(temp, "missing.json"),
		"--output", output,
	}, &stdout, &stderr)
	if code != 0 {
		t.Fatalf("exit %d: %s", code, stderr.String())
	}
	if !strings.Contains(stderr.String(), "WARNING") {
		t.Errorf("missing warning: %s", stderr.String())
	}
}
