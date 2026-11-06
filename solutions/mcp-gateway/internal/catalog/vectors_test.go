package catalog

import (
	"encoding/json"
	"testing"

	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/golden"
)

func TestResolveVectors(t *testing.T) {
	path := golden.RelativePath(t, "solutions/mcp-gateway/testdata/vectors/resolve.json")
	golden.Run(t, path, func(t *testing.T, file golden.File, input json.RawMessage) (any, error) {
		var in struct {
			Name string `json:"name"`
		}
		if err := json.Unmarshal(input, &in); err != nil {
			return nil, err
		}
		cat, err := Parse([]byte(file.Catalog))
		if err != nil {
			return map[string]any{"error": err.Error()}, nil
		}
		service, err := cat.Resolve(in.Name)
		if err != nil {
			return map[string]any{"error": err.Error()}, nil
		}
		return map[string]any{"service": service.ID}, nil
	})
}

func TestCatalogErrorVectors(t *testing.T) {
	path := golden.RelativePath(t, "solutions/mcp-gateway/testdata/vectors/catalog_errors.json")
	golden.Run(t, path, func(t *testing.T, _ golden.File, input json.RawMessage) (any, error) {
		var in struct {
			YAML string `json:"yaml"`
		}
		if err := json.Unmarshal(input, &in); err != nil {
			return nil, err
		}
		if _, err := Parse([]byte(in.YAML)); err != nil {
			return map[string]any{"error": err.Error()}, nil
		}
		return map[string]any{"ok": true}, nil
	})
}
