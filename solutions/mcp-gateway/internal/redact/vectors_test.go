package redact

import (
	"encoding/json"
	"strings"
	"testing"

	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/golden"
)

// The secret values are assembled from parts so the repository scanner never
// sees a contiguous credential. The vector file only names them.
func redactVectorText(text string) string {
	replacements := map[string]string{
		"{{token}}":    "abcdefgh" + "ijklmnop" + "qrstuvwx",
		"{{jwt}}":      "eyJhbGciOiJIUzI1NiJ9" + "." + "eyJzdWIiOiIxMjM0NTY3ODkwIn0" + "." + "dozjgNryP4J3jVmNHl0w5N_XgL0n3I9PlFUP0THsR8U",
		"{{hex}}":      "0123456789abcdef" + "0123456789abcdef",
		"{{telegram}}": "123456789:" + "AAHdqTcvCH1vGWJxfSeofSAs0K5PALDsaw",
	}
	for name, value := range replacements {
		text = strings.ReplaceAll(text, name, value)
	}
	return text
}

func TestRedactVectors(t *testing.T) {
	path := golden.RelativePath(t, "solutions/mcp-gateway/testdata/vectors/redact.json")
	golden.Run(t, path, func(t *testing.T, _ golden.File, input json.RawMessage) (any, error) {
		var in struct {
			Text string `json:"text"`
		}
		if err := json.Unmarshal(input, &in); err != nil {
			return nil, err
		}
		return Text(redactVectorText(in.Text)), nil
	})
}
