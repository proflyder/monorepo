// Package runtimevalues splits `helm template` output and the private runtime
// values into the two ownership domains of the runtime chart.
//
// Storage (PersistentVolume/PersistentVolumeClaim) is cluster-scoped and
// applied by kubectl; everything else is the Helm release. The split replaces
// the PyYAML heredoc in `projects/home/kubernetes/apply-runtime.sh`.
package runtimevalues

import (
	"bytes"
	"errors"
	"fmt"
	"io"
	"os"

	yaml "go.yaml.in/yaml/v2"
)

// StorageKind reports whether doc belongs to the kubectl-applied storage half.
func StorageKind(doc yaml.MapSlice) bool {
	kind := stringValue(doc, "kind")
	return kind == "PersistentVolume" || kind == "PersistentVolumeClaim"
}

// SplitRender reads a multi-document Helm render and returns the storage
// documents as a standalone YAML stream.
func SplitRender(path string) ([]byte, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	decoder := yaml.NewDecoder(bytes.NewReader(raw))
	var storage []yaml.MapSlice
	for {
		var doc yaml.MapSlice
		err := decoder.Decode(&doc)
		if err != nil {
			if errors.Is(err, io.EOF) {
				break
			}
			return nil, fmt.Errorf("%s: %w", path, err)
		}
		if len(doc) == 0 {
			continue
		}
		if StorageKind(doc) {
			storage = append(storage, doc)
		}
	}
	var output bytes.Buffer
	for index, doc := range storage {
		encoded, err := yaml.Marshal(doc)
		if err != nil {
			return nil, err
		}
		if index > 0 {
			output.WriteString("---\n")
		}
		output.Write(encoded)
	}
	return output.Bytes(), nil
}

// WorkloadValues reads the private values and drops the cluster-scoped storage
// block.
func WorkloadValues(path string) (yaml.MapSlice, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	values := yaml.MapSlice{}
	if len(bytes.TrimSpace(raw)) > 0 {
		if err := yaml.Unmarshal(raw, &values); err != nil {
			return nil, fmt.Errorf("%s: %w", path, err)
		}
	}
	result := make(yaml.MapSlice, 0, len(values))
	for _, item := range values {
		if fmt.Sprint(item.Key) == "storage" {
			continue
		}
		result = append(result, item)
	}
	return result, nil
}

// PinHealthExporter rewrites the exporter workload so the chart no longer needs
// the Python ConfigMap: it pins the image, sets the binary as the command and
// removes the script volume mount. The rest of the private values (resources,
// env, securityContext) are preserved.
func PinHealthExporter(values yaml.MapSlice, image, workloadName string) (yaml.MapSlice, error) {
	if image == "" {
		return values, nil
	}
	workloads, ok := mappingValue(values, "workloads")
	if !ok {
		return values, fmt.Errorf("values: workloads must be a mapping")
	}
	workload, ok := mappingValue(workloads, workloadName)
	if !ok {
		// No exporter workload in these values (for example a cluster that does
		// not run it yet): nothing to pin, keep the document unchanged.
		return values, nil
	}
	rawContainers, present := valueOf(workload, "containers")
	containers, ok := rawContainers.([]interface{})
	if !present || !ok || len(containers) == 0 {
		return values, fmt.Errorf("values: workloads.%s.containers must be a non-empty list", workloadName)
	}
	container, ok := containers[0].(yaml.MapSlice)
	if !ok {
		return values, fmt.Errorf("values: workloads.%s.containers[0] must be a mapping", workloadName)
	}
	container = setKey(container, "image", image)
	container = setKey(container, "command", []interface{}{"/usr/local/bin/health-exporter"})
	container = setKey(container, "volumeMounts", []interface{}{})
	containers[0] = container
	workload = setKey(workload, "containers", containers)
	workload, _ = deleteKey(workload, "volumes")
	workloads = setKey(workloads, workloadName, workload)
	return setKey(values, "workloads", workloads), nil
}

// Write renders data to path with mode 600.
func Write(path string, data []byte) error {
	return os.WriteFile(path, data, 0o600)
}

// Marshal serialises the runtime values.
func Marshal(values yaml.MapSlice) ([]byte, error) {
	return yaml.Marshal(values)
}

// --- ordered-map helpers -------------------------------------------------

func stringValue(mapping yaml.MapSlice, key string) string {
	for _, item := range mapping {
		if fmt.Sprint(item.Key) == key {
			text, _ := item.Value.(string)
			return text
		}
	}
	return ""
}

func valueOf(mapping yaml.MapSlice, key string) (interface{}, bool) {
	for _, item := range mapping {
		if fmt.Sprint(item.Key) == key {
			return item.Value, true
		}
	}
	return nil, false
}

func mappingValue(mapping yaml.MapSlice, key string) (yaml.MapSlice, bool) {
	for _, item := range mapping {
		if fmt.Sprint(item.Key) != key {
			continue
		}
		value, ok := item.Value.(yaml.MapSlice)
		return value, ok
	}
	return nil, false
}

func setKey(mapping yaml.MapSlice, key string, value interface{}) yaml.MapSlice {
	for index, item := range mapping {
		if fmt.Sprint(item.Key) == key {
			mapping[index].Value = value
			return mapping
		}
	}
	return append(mapping, yaml.MapItem{Key: key, Value: value})
}

func deleteKey(mapping yaml.MapSlice, key string) (yaml.MapSlice, bool) {
	for index, item := range mapping {
		if fmt.Sprint(item.Key) != key {
			continue
		}
		return append(mapping[:index:index], mapping[index+1:]...), true
	}
	return mapping, false
}
