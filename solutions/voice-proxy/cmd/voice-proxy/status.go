// Status prints a redacted JSON summary of the running configuration. It is the
// distroless replacement for the OPERATIONS `sh -c curl` diagnostics: switches,
// entity ids and the listen address, but never a token value.
package main

import (
	"encoding/json"
	"fmt"
	"io"
	"os"

	"ricardo.local/monorepo/solutions/voice-proxy/internal/config"
)

func runStatus() int {
	cfg, err := config.Load()
	if err != nil {
		fmt.Fprintln(os.Stderr, "status: configuration:", err)
		return 1
	}
	if err := writeStatus(os.Stdout, cfg); err != nil {
		fmt.Fprintln(os.Stderr, "status: encode:", err)
		return 1
	}
	return 0
}

func writeStatus(out io.Writer, cfg config.Config) error {
	enc := json.NewEncoder(out)
	enc.SetIndent("", "  ")
	enc.SetEscapeHTML(false)
	return enc.Encode(cfg.Redact())
}
