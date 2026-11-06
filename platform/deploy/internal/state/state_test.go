package state

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/proflyder/monorepo/platform/deploy/internal/kube"
)

type fakeKubectl struct {
	generations map[string]int
	calls       [][]string
	patch       map[string]interface{}
}

func (fake *fakeKubectl) run(args []string) kube.Result {
	fake.calls = append(fake.calls, args)
	if len(args) >= 3 && args[0] == "-n" && args[1] == "home-assistant" && args[2] == "get" {
		name := args[4]
		value, ok := fake.generations[name]
		if !ok {
			return kube.Result{Code: 1, Stderr: "not found"}
		}
		return kube.Result{Stdout: intToString(value)}
	}
	if len(args) >= 3 && args[2] == "replace" {
		path := args[indexOf(args, "-f")+1]
		raw, err := os.ReadFile(path)
		if err != nil {
			return kube.Result{Code: 1, Stderr: err.Error()}
		}
		_ = json.Unmarshal(raw, &fake.patch)
		return kube.Result{Stdout: "replaced"}
	}
	switch args[2] {
	case "delete":
		return kube.Result{Stdout: "deleted"}
	case "rollout":
		return kube.Result{Stdout: "ready"}
	}
	return kube.Result{Code: 1, Stderr: "unexpected call: " + strings.Join(args, " ")}
}

func (fake *fakeKubectl) verbs() []string {
	verbs := []string{}
	for _, args := range fake.calls {
		verbs = append(verbs, args[2])
	}
	return verbs
}

func intToString(value int) string {
	raw, _ := json.Marshal(value)
	return string(raw)
}

func indexOf(values []string, wanted string) int {
	for index, value := range values {
		if value == wanted {
			return index
		}
	}
	return -1
}

func beforeObject(generation int, image string) map[string]interface{} {
	return map[string]interface{}{
		"metadata": map[string]interface{}{"name": "scheduler", "generation": float64(generation)},
		"spec": map[string]interface{}{
			"replicas": float64(1),
			"template": map[string]interface{}{
				"spec": map[string]interface{}{
					"containers": []interface{}{
						map[string]interface{}{"name": "scheduler", "image": image},
					},
				},
			},
		},
	}
}

func writeBefore(t *testing.T, directory string, object map[string]interface{}) {
	t.Helper()
	raw, err := json.Marshal(object)
	if err != nil {
		t.Fatalf("marshal before: %v", err)
	}
	if err := os.WriteFile(filepath.Join(directory, "scheduler.before.json"), raw, 0o644); err != nil {
		t.Fatalf("write before: %v", err)
	}
}

func TestNoMarkersMeansNoCalls(t *testing.T) {
	fake := &fakeKubectl{generations: map[string]int{}}
	messages := Rollback(t.TempDir(), "home-assistant", fake.run)
	if len(fake.calls) != 0 {
		t.Fatalf("unexpected kubectl calls: %v", fake.calls)
	}
	if len(messages) != 1 || !strings.Contains(messages[0], "nothing to roll back") {
		t.Fatalf("unexpected messages: %v", messages)
	}
}

func TestApplyFailureBeforeMarkerDoesNotRollBack(t *testing.T) {
	directory := t.TempDir()
	writeBefore(t, directory, beforeObject(3, "old"))
	fake := &fakeKubectl{generations: map[string]int{"scheduler": 4}}
	Rollback(directory, "home-assistant", fake.run)
	if len(fake.calls) != 0 {
		t.Fatalf("unexpected kubectl calls: %v", fake.calls)
	}
}

func TestUnchangedGenerationIsNotRolledBack(t *testing.T) {
	directory := t.TempDir()
	writeBefore(t, directory, beforeObject(3, "old"))
	if err := os.WriteFile(filepath.Join(directory, "scheduler.applied"), nil, 0o644); err != nil {
		t.Fatal(err)
	}
	fake := &fakeKubectl{generations: map[string]int{"scheduler": 3}}
	messages := Rollback(directory, "home-assistant", fake.run)
	if strings.Contains(strings.Join(fake.verbs(), ","), "replace") {
		t.Fatalf("replace was called: %v", fake.verbs())
	}
	if len(messages) != 1 || !strings.Contains(messages[0], "did not change") {
		t.Fatalf("unexpected messages: %v", messages)
	}
}

func TestAppliedChangeIsRestoredToTheRecordedObject(t *testing.T) {
	before := beforeObject(7, "ghcr.io/proflyder/scheduler@sha256:"+strings.Repeat("b", 64))
	metadata := before["metadata"].(map[string]interface{})
	metadata["resourceVersion"] = "42"
	metadata["uid"] = "uid-1"
	before["status"] = map[string]interface{}{"replicas": float64(1)}
	directory := t.TempDir()
	writeBefore(t, directory, before)
	if err := os.WriteFile(filepath.Join(directory, "scheduler.applied"), nil, 0o644); err != nil {
		t.Fatal(err)
	}
	fake := &fakeKubectl{generations: map[string]int{"scheduler": 8}}
	messages := Rollback(directory, "home-assistant", fake.run)
	verbs := strings.Join(fake.verbs(), ",")
	if !strings.Contains(verbs, "replace") {
		t.Fatalf("replace was not called: %v", fake.verbs())
	}
	if fake.patch == nil {
		t.Fatal("replace payload was not recorded")
	}
	if _, ok := fake.patch["status"]; ok {
		t.Fatal("status was sent back")
	}
	patchMetadata := fake.patch["metadata"].(map[string]interface{})
	if _, ok := patchMetadata["resourceVersion"]; ok {
		t.Fatal("resourceVersion was sent back")
	}
	if _, ok := patchMetadata["uid"]; ok {
		t.Fatal("uid was sent back")
	}
	if patchMetadata["name"] != "scheduler" {
		t.Fatalf("metadata.name = %v", patchMetadata["name"])
	}
	if !strings.Contains(verbs, "rollout") {
		t.Fatalf("rollout status was not checked: %v", fake.verbs())
	}
	if len(messages) != 1 || !strings.Contains(messages[0], "restored to generation 7") {
		t.Fatalf("unexpected messages: %v", messages)
	}
}

func TestFirstInstallIsDeleted(t *testing.T) {
	directory := t.TempDir()
	for _, name := range []string{"ha-bridge.absent", "ha-bridge.applied"} {
		if err := os.WriteFile(filepath.Join(directory, name), nil, 0o644); err != nil {
			t.Fatal(err)
		}
	}
	fake := &fakeKubectl{generations: map[string]int{"ha-bridge": 1}}
	messages := Rollback(directory, "home-assistant", fake.run)
	verbs := strings.Join(fake.verbs(), ",")
	if !strings.Contains(verbs, "delete") {
		t.Fatalf("delete was not called: %v", fake.verbs())
	}
	if len(messages) != 1 || !strings.Contains(messages[0], "deleted") {
		t.Fatalf("unexpected messages: %v", messages)
	}
}

func TestMarkerWithoutStateIsSkipped(t *testing.T) {
	directory := t.TempDir()
	if err := os.WriteFile(filepath.Join(directory, "scheduler.applied"), nil, 0o644); err != nil {
		t.Fatal(err)
	}
	fake := &fakeKubectl{generations: map[string]int{"scheduler": 4}}
	messages := Rollback(directory, "home-assistant", fake.run)
	if strings.Contains(strings.Join(fake.verbs(), ","), "replace") {
		t.Fatalf("replace was called: %v", fake.verbs())
	}
	if len(messages) != 1 || !strings.Contains(messages[0], "no recorded state") {
		t.Fatalf("unexpected messages: %v", messages)
	}
}

// TestRecordAndMarkKeepTheMarkerContract pins the files deploy.sh relies on.
func TestRecordAndMarkKeepTheMarkerContract(t *testing.T) {
	directory := t.TempDir()
	live := `{"metadata":{"name":"scheduler","generation":3},"spec":{}}`
	calls := [][]string{}
	run := func(args []string) kube.Result {
		calls = append(calls, args)
		switch {
		case len(args) >= 3 && args[2] == "get" && indexOf(args, "-o") == -1:
			return kube.Result{Code: 0}
		case len(args) >= 3 && args[2] == "get":
			return kube.Result{Stdout: live}
		}
		return kube.Result{Code: 1}
	}
	if err := Record(directory, "home-assistant", "scheduler", run); err != nil {
		t.Fatalf("record: %v", err)
	}
	raw, err := os.ReadFile(filepath.Join(directory, "scheduler.before.json"))
	if err != nil {
		t.Fatalf("before marker: %v", err)
	}
	if string(raw) != live {
		t.Fatalf("before marker = %q, want the exact kubectl stdout", string(raw))
	}
	if _, err := os.Stat(filepath.Join(directory, "scheduler.applied")); !os.IsNotExist(err) {
		t.Fatalf("record must clear the applied marker: %v", err)
	}
	if err := Mark(directory, "scheduler"); err != nil {
		t.Fatalf("mark: %v", err)
	}
	if _, err := os.Stat(filepath.Join(directory, "scheduler.applied")); err != nil {
		t.Fatalf("applied marker: %v", err)
	}
}

// TestRecordAbsentWritesTheAbsentMarker covers a first install: no before.json.
func TestRecordAbsentWritesTheAbsentMarker(t *testing.T) {
	directory := t.TempDir()
	run := func(args []string) kube.Result {
		return kube.Result{Code: 1, Stderr: "not found"}
	}
	if err := Record(directory, "home-assistant", "scheduler", run); err != nil {
		t.Fatalf("record: %v", err)
	}
	if _, err := os.Stat(filepath.Join(directory, "scheduler.absent")); err != nil {
		t.Fatalf("absent marker: %v", err)
	}
	if _, err := os.Stat(filepath.Join(directory, "scheduler.before.json")); !os.IsNotExist(err) {
		t.Fatalf("no before marker expected for an absent Deployment: %v", err)
	}
}
