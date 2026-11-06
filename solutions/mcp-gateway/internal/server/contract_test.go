package server

import (
	"context"
	"io"
	"net/http"
	"testing"

	"github.com/modelcontextprotocol/go-sdk/mcp"

	"github.com/proflyder/monorepo/packages/go/contracttest"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/catalog"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/group"
	"github.com/proflyder/monorepo/solutions/mcp-gateway/internal/tools"
)

const contractCatalog = `
overview: |
  ## Contract topology
  One fake service.
services:
  - id: scheduler
    aliases:
      ru: [планировщик]
      en: [scheduler]
    kind: k8s
    namespace: home
    workload: {kind: Deployment, name: scheduler}
    containers: [scheduler]
    restart:
      allowed: true
      warning: Contract restart warning.
`

// TestContractFixtures replays the gateway's HTTP contract: MCP Streamable
// HTTP and the separate voice token. The runner owns the transcript; this file
// composes a real MCP server from the infra.read group and a fake catalog.
func TestContractFixtures(t *testing.T) {
	contracttest.Run(t, "solutions/mcp-gateway/internal/server/testdata/contract", newContractServer,
		contracttest.WithNormalizer(dropSDKFields))
}

func newContractServer(t *testing.T, _ string) http.Handler {
	t.Helper()
	cat, err := catalog.Parse([]byte(contractCatalog))
	if err != nil {
		t.Fatalf("catalog: %v", err)
	}
	deps := &tools.Deps{Catalog: cat}
	mcpServer := mcp.NewServer(&mcp.Implementation{
		Name:    "mcp-gateway",
		Title:   "Infra gateway",
		Version: "contract",
	}, &mcp.ServerOptions{Logger: discardLogger()})
	registry := group.NewRegistry(&tools.ReadGroup{Deps: deps})
	selected, err := registry.Select([]string{tools.GroupRead})
	if err != nil {
		t.Fatalf("select groups: %v", err)
	}
	registry.RegisterAll(mcpServer, selected)

	stream := mcp.NewStreamableHTTPHandler(
		func(*http.Request) *mcp.Server { return mcpServer },
		&mcp.StreamableHTTPOptions{Logger: discardLogger()},
	)
	voice := http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		_, _ = io.WriteString(w, `{"status":"ok"}`)
	})
	return New(Options{
		AuthToken:  "mcp-contract-token",
		VoiceToken: "voice-contract-token",
		MCP:        stream,
		Voice:      voice,
		Ready:      func(context.Context) error { return nil },
		Logger:     discardLogger(),
	}).Handler()
}

// dropSDKFields keeps the fixtures language-neutral: the Go SDK's ttlMs,
// cacheScope, structuredContent and idempotentHint:false are optional and are
// accepted by the semantic comparison but not required from another SDK.
func dropSDKFields(_ string, value any) any {
	switch typed := value.(type) {
	case map[string]any:
		for _, key := range []string{"ttlMs", "cacheScope", "structuredContent"} {
			delete(typed, key)
		}
		if idempotent, ok := typed["idempotentHint"].(bool); ok && !idempotent {
			delete(typed, "idempotentHint")
		}
		for key, item := range typed {
			typed[key] = dropSDKFields("", item)
		}
		return typed
	case []any:
		for index, item := range typed {
			typed[index] = dropSDKFields("", item)
		}
		return typed
	default:
		return value
	}
}
