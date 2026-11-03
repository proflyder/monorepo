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
		return fmt.Sprintf("%s: created from manifest", options.Name), nil
	}

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
	return fmt.Sprintf("%s: converged to manifest", options.Name), nil
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
