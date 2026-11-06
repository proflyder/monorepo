// Package state owns the deploy rollback markers.
//
// `deploy.sh` records, immediately before it applies a change:
//
//	<name>.before.json   the full Deployment object as it was
//	<name>.absent        the Deployment did not exist
//
// and writes `<name>.applied` only after the apply/patch succeeded. This is a
// port of the `record_before`/`mark_applied` helpers in the deploy scripts and
// of `projects/home/kubernetes/scheduler/rollback.py`; the marker contract is
// byte-identical so a mixed run (an old deploy.sh and the new binary, or the
// other way round) stays safe.
//
// Rollback restores from that record instead of running `kubectl rollout undo`:
//
//   - a failure before `apply` (image build/push, rendering, preflight) leaves
//     no `.applied` marker, so nothing is rolled back;
//   - a failure after `apply` restores the exact remembered object (image, env,
//     replicas, probes, volumes) with a full replace — not "one revision back",
//     which could undo an unrelated change made by an earlier deploy step, and
//     not a merge patch, which cannot remove fields added by the failed deploy;
//   - if the Deployment generation did not change, no request is sent at all.
package state

import (
	"encoding/json"
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"

	"github.com/proflyder/monorepo/platform/deploy/internal/kube"
)

// Record remembers the live Deployment name (or its absence) and clears the
// applied marker. stateDir must already exist (deploy.sh recreates it).
func Record(stateDir, namespace, name string, run kube.Runner) error {
	if result := run([]string{"-n", namespace, "get", "deployment", name}); result.Code != 0 {
		if err := os.WriteFile(marker(stateDir, name, ".absent"), nil, 0o644); err != nil {
			return err
		}
	} else {
		live := run([]string{"-n", namespace, "get", "deployment", name, "-o", "json"})
		if live.Code != 0 {
			return fmt.Errorf("%s: cannot record deployment: %s", name, strip(live.Stderr))
		}
		if err := os.WriteFile(marker(stateDir, name, ".before.json"), []byte(live.Stdout), 0o644); err != nil {
			return err
		}
	}
	return removeIfExists(marker(stateDir, name, ".applied"))
}

// Mark records that the Deployment change was applied by this job.
func Mark(stateDir, name string) error {
	return os.WriteFile(marker(stateDir, name, ".applied"), nil, 0o644)
}

// Rollback restores every Deployment marked as applied.
func Rollback(stateDir, namespace string, run kube.Runner) []string {
	info, err := os.Stat(stateDir)
	if err != nil || !info.IsDir() {
		return []string{"no deploy state recorded, nothing to roll back"}
	}
	matches, err := filepath.Glob(filepath.Join(stateDir, "*.applied"))
	if err != nil {
		return []string{"no deploy state recorded, nothing to roll back"}
	}
	markers := []string{}
	for _, match := range matches {
		info, err := os.Stat(match)
		if err == nil && !info.IsDir() {
			markers = append(markers, match)
		}
	}
	if len(markers) == 0 {
		return []string{"no deployment was applied by this job, nothing to roll back"}
	}
	sort.Strings(markers)
	messages := make([]string, 0, len(markers))
	for _, match := range markers {
		name := strings.TrimSuffix(filepath.Base(match), ".applied")
		messages = append(messages, restoreOne(namespace, stateDir, name, run))
	}
	return messages
}

// RestoreObject strips read-only fields so the recorded object can be PUT back
// as-is.
func RestoreObject(before map[string]interface{}) map[string]interface{} {
	object := deepCopy(before)
	delete(object, "status")
	metadata, _ := object["metadata"].(map[string]interface{})
	if metadata == nil {
		return object
	}
	for _, key := range []string{"managedFields", "generation", "creationTimestamp", "selfLink", "uid"} {
		delete(metadata, key)
	}
	// An empty resourceVersion makes the update unconditional; the recorded
	// revision is gone by definition after a failed deploy.
	delete(metadata, "resourceVersion")
	return object
}

func restoreOne(namespace, stateDir, name string, run kube.Runner) string {
	applied := marker(stateDir, name, ".applied")
	beforePath := marker(stateDir, name, ".before.json")
	absent := marker(stateDir, name, ".absent")

	if _, err := os.Stat(applied); err != nil {
		return fmt.Sprintf("%s: not applied by this job, skip", name)
	}

	if _, err := os.Stat(absent); err == nil {
		result := run([]string{"-n", namespace, "delete", "deployment", name, "--ignore-not-found"})
		if result.Code != 0 {
			return fmt.Sprintf("%s: delete failed: %s", name, strip(result.Stderr))
		}
		return fmt.Sprintf("%s: was absent before, deleted", name)
	}

	raw, err := os.ReadFile(beforePath)
	if err != nil {
		return fmt.Sprintf("%s: applied but no recorded state, skip", name)
	}
	var before map[string]interface{}
	if err := json.Unmarshal(raw, &before); err != nil {
		return fmt.Sprintf("%s: recorded state has no generation, skip", name)
	}
	beforeGeneration, ok := generationField(before)
	if !ok {
		return fmt.Sprintf("%s: recorded state has no generation, skip", name)
	}

	current, ok := generation(namespace, name, run)
	if !ok {
		return fmt.Sprintf("%s: deployment not found, skip", name)
	}
	if current <= beforeGeneration {
		return fmt.Sprintf("%s: generation %d did not change (was %d), skip", name, current, beforeGeneration)
	}

	patch := RestoreObject(before)
	payload, err := json.Marshal(patch)
	if err != nil {
		return fmt.Sprintf("%s: restore replace failed: %s", name, err)
	}
	handle, err := os.CreateTemp("", "*.json")
	if err != nil {
		return fmt.Sprintf("%s: restore replace failed: %s", name, err)
	}
	path := handle.Name()
	if _, err := handle.Write(payload); err != nil {
		handle.Close()
		os.Remove(path)
		return fmt.Sprintf("%s: restore replace failed: %s", name, err)
	}
	handle.Close()
	result := run([]string{"-n", namespace, "replace", "-f", path})
	os.Remove(path)
	if result.Code != 0 {
		return fmt.Sprintf("%s: restore replace failed: %s", name, strip(result.Stderr))
	}

	rollout := run([]string{"-n", namespace, "rollout", "status", "deployment/" + name, "--timeout=180s"})
	if rollout.Code != 0 {
		return fmt.Sprintf("%s: restored to generation %d, rollout not ready: %s",
			name, beforeGeneration, strip(rollout.Stderr))
	}
	return fmt.Sprintf("%s: restored to generation %d", name, beforeGeneration)
}

func generation(namespace, name string, run kube.Runner) (int, bool) {
	result := run([]string{"-n", namespace, "get", "deployment", name, "-o", "jsonpath={.metadata.generation}"})
	if result.Code != 0 {
		return 0, false
	}
	text := strings.TrimSpace(result.Stdout)
	if text == "" {
		return 0, false
	}
	value, err := strconv.Atoi(text)
	if err != nil {
		return 0, false
	}
	return value, true
}

func generationField(before map[string]interface{}) (int, bool) {
	metadata, _ := before["metadata"].(map[string]interface{})
	if metadata == nil {
		return 0, false
	}
	switch value := metadata["generation"].(type) {
	case float64:
		return int(value), true
	case int:
		return value, true
	case json.Number:
		parsed, err := value.Int64()
		if err != nil {
			return 0, false
		}
		return int(parsed), true
	default:
		return 0, false
	}
}

func deepCopy(value map[string]interface{}) map[string]interface{} {
	encoded, _ := json.Marshal(value)
	var result map[string]interface{}
	if err := json.Unmarshal(encoded, &result); err != nil {
		return nil
	}
	return result
}

func marker(stateDir, name, suffix string) string {
	return filepath.Join(stateDir, name+suffix)
}

func removeIfExists(path string) error {
	err := os.Remove(path)
	if errors.Is(err, os.ErrNotExist) {
		return nil
	}
	return err
}

func strip(value string) string {
	return strings.TrimRight(value, " \t\r\n")
}
