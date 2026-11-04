package render

import (
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"
)

// renderedAnnotations is what the two build-related annotations carry after a
// render: the content-addressed pod-template value and the git-revision value
// kept on the Deployment for traceability.
type renderedAnnotations struct {
	rollout string
	build   string
}

func parseAnnotations(text string) renderedAnnotations {
	var parsed renderedAnnotations
	for _, line := range strings.Split(text, "\n") {
		trimmed := strings.TrimSpace(line)
		if value, ok := strings.CutPrefix(trimmed, "ricardo.dev/rollout: "); ok {
			parsed.rollout = value
		}
		if value, ok := strings.CutPrefix(trimmed, "ricardo.dev/build: "); ok {
			parsed.build = value
		}
	}
	return parsed
}

// renderScheduler renders the scheduler unit exactly like deploy.sh does, with
// the content-addressed rollout id, and returns the annotations from the
// scheduler Deployment. overrides replace individual values.
func renderScheduler(t *testing.T, overrides map[string]string) renderedAnnotations {
	t.Helper()
	testCase := goldenCases[0]
	values := valuesFor(t, testCase)
	for key, value := range overrides {
		values[key] = value
	}
	sets := make([]string, 0, len(values))
	for key, value := range values {
		sets = append(sets, key+"="+value)
	}
	sort.Strings(sets)
	files := make([]string, 0, len(testCase.files))
	for _, file := range testCase.files {
		files = append(files, locate(t, file))
	}

	outDir := t.TempDir()
	if err := Run(Options{
		Sets:       sets,
		OutDir:     outDir,
		Files:      files,
		Unit:       testCase.preset,
		RolloutKey: RolloutKey,
	}); err != nil {
		t.Fatalf("render: %v", err)
	}
	raw, err := os.ReadFile(filepath.Join(outDir, "scheduler.yaml"))
	if err != nil {
		t.Fatalf("read rendered scheduler: %v", err)
	}
	return parseAnnotations(string(raw))
}

// TestContentAddressedRollout is the regression test for the deploy-home
// incident: a new git revision alone rolled every pod even though the image
// digests were identical.
func TestContentAddressedRollout(t *testing.T) {
	first := renderScheduler(t, map[string]string{"BUILD_ID": "aa6239e-0123456789ab"})
	second := renderScheduler(t, map[string]string{"BUILD_ID": "e5c6316-0123456789ab"})

	if first.rollout == "" || len(first.rollout) != 12 {
		t.Fatalf("rollout annotation = %q, want 12 hex characters", first.rollout)
	}
	if first.rollout != second.rollout {
		t.Fatalf("git revision changed the pod-template annotation: %q != %q",
			first.rollout, second.rollout)
	}
	if first.build == second.build {
		t.Fatalf("Deployment traceability annotation did not change with the revision: %q", first.build)
	}
	if !strings.Contains(first.build, "aa6239e") || !strings.Contains(second.build, "e5c6316") {
		t.Fatalf("build annotations do not carry the revision: %q / %q", first.build, second.build)
	}

	// A changed image digest must still change what the pod template addresses.
	changedImage := renderScheduler(t, map[string]string{
		"SCHEDULER_IMAGE": image("ghcr.io/proflyder/scheduler", 9),
	})
	if changedImage.rollout == first.rollout {
		t.Fatal("changed image digest kept the same rollout annotation")
	}

	// Config that lands in the rendered pods must roll them too.
	changedConfig := renderScheduler(t, map[string]string{"HA_URL": "http://192.0.2.29:8123"})
	if changedConfig.rollout == first.rollout {
		t.Fatal("changed rendered config kept the same rollout annotation")
	}
}

func TestRunRejectsUnknownRolloutKey(t *testing.T) {
	testCase := goldenCases[0]
	files := make([]string, 0, len(testCase.files))
	for _, file := range testCase.files {
		files = append(files, locate(t, file))
	}
	err := Run(Options{
		Sets:       []string{"BUILD_ID=abc", "ROLLOUT_ID=abc", "SCHEDULER_IMAGE=" + image("ghcr.io/proflyder/scheduler", 0)},
		OutDir:     t.TempDir(),
		Files:      files,
		Unit:       testCase.preset,
		RolloutKey: "NOT_A_PLACEHOLDER",
	})
	if err == nil {
		t.Fatal("expected an error for a rollout key outside the unit allowlist")
	}
}
