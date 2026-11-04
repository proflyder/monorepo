// Package manifests contains the small manifest helpers the deploy scripts
// need: resolving ExternalSecret names and writing a bootstrap Secret.
package manifests

import (
	"bytes"
	"encoding/base64"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"os"
	"strings"

	yaml "go.yaml.in/yaml/v2"

	"ricardo.local/monorepo/platform/deploy/internal/envfile"
)

// ExternalSecretNames reads multi-document manifests and returns every
// ExternalSecret as `<namespace>/<name>`, in file and document order. It
// replaces the PyYAML heredoc in external-secrets/deploy.sh.
func ExternalSecretNames(paths []string) ([]string, error) {
	var names []string
	for _, path := range paths {
		raw, err := os.ReadFile(path)
		if err != nil {
			return nil, err
		}
		decoder := yaml.NewDecoder(bytes.NewReader(raw))
		for {
			var doc yaml.MapSlice
			if err := decoder.Decode(&doc); err != nil {
				if errors.Is(err, io.EOF) {
					break
				}
				return nil, fmt.Errorf("%s: %w", path, err)
			}
			if len(doc) == 0 || stringValue(doc, "kind") != "ExternalSecret" {
				continue
			}
			metadata, ok := mappingValue(doc, "metadata")
			if !ok {
				return nil, fmt.Errorf("%s: ExternalSecret without metadata", path)
			}
			namespace := stringValue(metadata, "namespace")
			name := stringValue(metadata, "name")
			if namespace == "" || name == "" {
				return nil, fmt.Errorf("%s: ExternalSecret needs metadata.namespace and metadata.name", path)
			}
			names = append(names, namespace+"/"+name)
		}
	}
	return names, nil
}

// WriteNames writes one name per line to path.
func WriteNames(path string, names []string) error {
	return os.WriteFile(path, []byte(strings.Join(names, "\n")+"\n"), 0o644)
}

// OpaqueSecret builds a `type: Opaque` Secret JSON document with one key whose
// value is read from a dotenv-style file. It replaces the ingress bootstrap
// heredoc; the value never touches the filesystem outside the private file.
func OpaqueSecret(name, namespace, secretKey, envFile, envKey string) ([]byte, error) {
	value, present, err := envfile.Lookup(envFile, envKey)
	if err != nil {
		return nil, err
	}
	if !present || value == "" {
		return nil, fmt.Errorf("%s is missing in %s", envKey, envFile)
	}
	document := map[string]interface{}{
		"apiVersion": "v1",
		"kind":       "Secret",
		"metadata": map[string]interface{}{
			"name":      name,
			"namespace": namespace,
		},
		"type": "Opaque",
		"data": map[string]interface{}{
			secretKey: base64.StdEncoding.EncodeToString([]byte(value)),
		},
	}
	return json.Marshal(document)
}

func stringValue(mapping yaml.MapSlice, key string) string {
	for _, item := range mapping {
		if fmt.Sprint(item.Key) == key {
			text, _ := item.Value.(string)
			return text
		}
	}
	return ""
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
