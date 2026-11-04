// Package converge replaces a live Deployment with its rendered manifest.
//
// `kubectl apply`/`kubectl patch` only add or update the fields they carry: a
// volume or volumeMount that an earlier ad-hoc patch added stays in the live
// object forever (the 2026-10 hostPath `/opt/ricardo/current/bin` drift). This
// helper renders the desired object and replaces the live Deployment with it,
// so fields absent from the manifest are actually removed.
//
// It is a port of `projects/home/kubernetes/scheduler/converge.py` and keeps
// the same kubectl argv, the same carry-over rules and the same messages:
//
//   - PreserveReplicas keeps the current replica count (mode/scaling is manual);
//   - PreserveEnv keeps the current container env/envFrom (execute flags);
//   - everything else (pod template, volumes, probes, resources, image) comes
//     from the rendered manifest;
//   - the current resourceVersion is reused so a concurrent change fails the
//     replace instead of being overwritten.
//
// Service/PVC documents in the same file are applied first; only the
// Deployment is converged.
package converge

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"reflect"
	"strconv"
	"strings"

	yaml "go.yaml.in/yaml/v2"

	"ricardo.local/monorepo/platform/deploy/internal/kube"
)

// Options is one converge request.
type Options struct {
	Kubectl          string
	Namespace        string
	Name             string
	File             string
	PreserveReplicas bool
	PreserveEnv      bool
	// SummaryFile, when set, receives one `<name>\t<status>` line after the
	// Deployment was applied: `created`, `unchanged` (the pod template did not
	// change, so no new ReplicaSet and no pod restart) or `rolled-out`. It is
	// read by the deploy-home job summary; an empty value disables the extra
	// kubectl call.
	SummaryFile string
}

// DesiredObjects parses every non-empty YAML document in the rendered file.
func DesiredObjects(path string) ([]map[string]interface{}, error) {
	handle, err := os.Open(path)
	if err != nil {
		return nil, err
	}
	defer handle.Close()
	decoder := yaml.NewDecoder(handle)
	objects := []map[string]interface{}{}
	for {
		var document interface{}
		err := decoder.Decode(&document)
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			return nil, fmt.Errorf("%s: %w", path, err)
		}
		normalized := normalize(document)
		object, ok := normalized.(map[string]interface{})
		if !ok || len(object) == 0 {
			// Python skips a document that is empty or a falsy empty mapping.
			continue
		}
		objects = append(objects, object)
	}
	return objects, nil
}

// DesiredDeployment returns the single Deployment document.
func DesiredDeployment(objects []map[string]interface{}, path string) (map[string]interface{}, error) {
	deployments := []map[string]interface{}{}
	for _, object := range objects {
		if object["kind"] == "Deployment" {
			deployments = append(deployments, object)
		}
	}
	if len(deployments) != 1 {
		return nil, fmt.Errorf("%s: expected exactly one Deployment, found %d", path, len(deployments))
	}
	return deployments[0], nil
}

// Converge drives kubectl and returns the human-readable result line.
func Converge(options Options, run kube.Runner) (string, error) {
	objects, err := DesiredObjects(options.File)
	if err != nil {
		return "", err
	}
	desired, err := DesiredDeployment(objects, options.File)
	if err != nil {
		return "", err
	}

	support := []map[string]interface{}{}
	for _, object := range objects {
		if object["kind"] != "Deployment" {
			support = append(support, object)
		}
	}
	if err := applySupportObjects(options, support, run); err != nil {
		return "", err
	}

	current := liveDeployment(options, run)
	if current == nil {
		path, cleanup, err := tempFile("*.yaml", serializeYAML(desired))
		if err != nil {
			return "", err
		}
		defer cleanup()
		result := run([]string{"-n", options.Namespace, "create", "-f", path})
		if result.Code != 0 {
			return "", fmt.Errorf("%s: create failed: %s", options.Name, strip(result.Stderr))
		}
		if err := recordSummary(options, "created"); err != nil {
			return "", err
		}
		return fmt.Sprintf("%s: created from manifest", options.Name), nil
	}

	before := generation(current)
	mergeRuntimeState(desired, current, options.PreserveReplicas, options.PreserveEnv)

	payload, err := json.Marshal(desired)
	if err != nil {
		return "", err
	}
	path, cleanup, err := tempFile("*.json", payload)
	if err != nil {
		return "", err
	}
	defer cleanup()
	result := run([]string{"-n", options.Namespace, "replace", "-f", path})
	if result.Code != 0 {
		return "", fmt.Errorf("%s: replace failed: %s", options.Name, strip(result.Stderr))
	}
	if err := recordSummary(options, rolloutStatus(options, before, run)); err != nil {
		return "", err
	}
	return fmt.Sprintf("%s: converged to manifest", options.Name), nil
}

// generation reads metadata.generation from a Deployment object. The API server
// increments it exactly when the desired state changes, and a pod template
// change is what creates a new ReplicaSet, so comparing it before and after an
// apply is the same signal `kubectl apply` uses to decide on a rollout.
func generation(object map[string]interface{}) string {
	var value interface{} = object
	for _, key := range []string{"metadata", "generation"} {
		mapping, ok := value.(map[string]interface{})
		if !ok {
			return ""
		}
		value = mapping[key]
	}
	if value == nil {
		return ""
	}
	if number, ok := value.(float64); ok {
		// json.Unmarshal decodes numbers as float64; format it without an
		// exponent so it matches kubectl's jsonpath output for large counts.
		return strconv.FormatFloat(number, 'f', -1, 64)
	}
	return fmt.Sprint(value)
}

// rolloutStatus compares the generation before the replace with the generation
// the API server reports afterwards. An absent or unreadable value is reported
// as `unknown` rather than guessing.
func rolloutStatus(options Options, before string, run kube.Runner) string {
	if options.SummaryFile == "" {
		return ""
	}
	result := run([]string{"-n", options.Namespace, "get", "deployment", options.Name,
		"-o", "jsonpath={.metadata.generation}"})
	if result.Code != 0 {
		return "unknown"
	}
	after := strings.TrimSpace(result.Stdout)
	switch {
	case before == "" || after == "":
		return "unknown"
	case before == after:
		return "unchanged"
	default:
		return "rolled-out"
	}
}

// recordSummary appends one `<name>\t<status>` line to the per-job summary.
func recordSummary(options Options, status string) error {
	if options.SummaryFile == "" || status == "" {
		return nil
	}
	handle, err := os.OpenFile(options.SummaryFile, os.O_APPEND|os.O_CREATE|os.O_WRONLY, 0o644)
	if err != nil {
		return fmt.Errorf("%s: open summary: %w", options.Name, err)
	}
	defer handle.Close()
	if _, err := fmt.Fprintf(handle, "%s\t%s\n", options.Name, status); err != nil {
		return fmt.Errorf("%s: write summary: %w", options.Name, err)
	}
	return nil
}

func applySupportObjects(options Options, objects []map[string]interface{}, run kube.Runner) error {
	if len(objects) == 0 {
		return nil
	}
	var documents []byte
	for index, object := range objects {
		if index > 0 {
			documents = append(documents, []byte("---\n")...)
		}
		encoded, err := yaml.Marshal(object)
		if err != nil {
			return err
		}
		documents = append(documents, encoded...)
	}
	path, cleanup, err := tempFile("*.yaml", documents)
	if err != nil {
		return err
	}
	defer cleanup()
	result := run([]string{"-n", options.Namespace, "apply", "-f", path})
	if result.Code != 0 {
		return fmt.Errorf("support objects: apply failed: %s", strip(result.Stderr))
	}
	return nil
}

func liveDeployment(options Options, run kube.Runner) map[string]interface{} {
	result := run([]string{"-n", options.Namespace, "get", "deployment", options.Name, "-o", "json"})
	if result.Code != 0 {
		return nil
	}
	var current map[string]interface{}
	if err := json.Unmarshal([]byte(result.Stdout), &current); err != nil {
		return nil
	}
	return current
}

func mergeRuntimeState(desired, current map[string]interface{}, preserveReplicas, preserveEnv bool) {
	desiredSpec, _ := desired["spec"].(map[string]interface{})
	if desiredSpec == nil {
		desiredSpec = map[string]interface{}{}
		desired["spec"] = desiredSpec
	}
	currentSpec, _ := current["spec"].(map[string]interface{})

	if preserveReplicas && currentSpec != nil {
		if replicas, ok := currentSpec["replicas"]; ok {
			desiredSpec["replicas"] = replicas
		}
	}

	if preserveEnv && currentSpec != nil {
		currentContainers := containers(currentSpec)
		desiredTemplate, _ := desiredSpec["template"].(map[string]interface{})
		desiredPod, _ := desiredTemplate["spec"].(map[string]interface{})
		if desiredPod != nil {
			if list, ok := desiredPod["containers"].([]interface{}); ok {
				for _, item := range list {
					container, ok := item.(map[string]interface{})
					if !ok {
						continue
					}
					name, _ := container["name"].(string)
					live, ok := currentContainers[name]
					if !ok {
						continue
					}
					for _, key := range []string{"env", "envFrom"} {
						// An empty live value means there is no runtime state to
						// keep; the manifest value stays in place then.
						if value, ok := live[key]; ok && truthy(value) {
							container[key] = value
						}
					}
				}
			}
		}
	}

	if version := nestedString(current, "metadata", "resourceVersion"); version != "" {
		metadata, _ := desired["metadata"].(map[string]interface{})
		if metadata == nil {
			metadata = map[string]interface{}{}
			desired["metadata"] = metadata
		}
		// The rendered manifest never carries a resourceVersion; the live one
		// wins so a concurrent change fails the replace.
		metadata["resourceVersion"] = version
	}
}

func containers(spec map[string]interface{}) map[string]map[string]interface{} {
	result := map[string]map[string]interface{}{}
	template, _ := spec["template"].(map[string]interface{})
	pod, _ := template["spec"].(map[string]interface{})
	list, _ := pod["containers"].([]interface{})
	for _, item := range list {
		container, ok := item.(map[string]interface{})
		if !ok {
			continue
		}
		name, _ := container["name"].(string)
		result[name] = container
	}
	return result
}

func nestedString(object map[string]interface{}, keys ...string) string {
	var value interface{} = object
	for _, key := range keys {
		mapping, ok := value.(map[string]interface{})
		if !ok {
			return ""
		}
		value = mapping[key]
	}
	text, _ := value.(string)
	return text
}

// truthy mirrors Python's truth test for the values we handle.
func truthy(value interface{}) bool {
	switch typed := value.(type) {
	case nil:
		return false
	case []interface{}:
		return len(typed) > 0
	case map[string]interface{}:
		return len(typed) > 0
	case string:
		return typed != ""
	case bool:
		return typed
	default:
		return !reflect.ValueOf(value).IsZero()
	}
}

// normalize converts yaml.v2's map[interface{}]interface{} into JSON-friendly
// maps. Kubernetes keys are strings, so a non-string key is stringified.
func normalize(value interface{}) interface{} {
	switch typed := value.(type) {
	case map[interface{}]interface{}:
		result := make(map[string]interface{}, len(typed))
		for key, item := range typed {
			result[fmt.Sprint(key)] = normalize(item)
		}
		return result
	case map[string]interface{}:
		result := make(map[string]interface{}, len(typed))
		for key, item := range typed {
			result[key] = normalize(item)
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

func serializeYAML(object map[string]interface{}) []byte {
	encoded, err := yaml.Marshal(object)
	if err != nil {
		return nil
	}
	return encoded
}

func tempFile(pattern string, content []byte) (string, func(), error) {
	handle, err := os.CreateTemp("", pattern)
	if err != nil {
		return "", func() {}, err
	}
	path := handle.Name()
	if _, err := handle.Write(content); err != nil {
		handle.Close()
		os.Remove(path)
		return "", func() {}, err
	}
	if err := handle.Close(); err != nil {
		os.Remove(path)
		return "", func() {}, err
	}
	return path, func() { os.Remove(path) }, nil
}

func strip(value string) string {
	for len(value) > 0 {
		last := value[len(value)-1]
		if last != ' ' && last != '\n' && last != '\r' && last != '\t' {
			break
		}
		value = value[:len(value)-1]
	}
	return value
}
