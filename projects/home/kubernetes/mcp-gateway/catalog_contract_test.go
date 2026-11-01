// Package mcpdeploy holds deploy contracts for the home Kubernetes deployment
// of solutions/mcp-gateway, next to the manifests they guard.
//
// The catalog is deployment data, so it must load with the gateway's real
// loader rather than a copy of its rules. The loader lives in the solution's
// internal packages, which a project may not import, so this test crosses the
// process boundary: it builds the production binary, starts it with the
// tracked catalog and expects /readyz to turn green (readiness means "the
// catalog is loaded"). A deliberately invalid copy must refuse to start.
//
// The catalog/RBAC correspondence and the deploy-script contract are asserted
// in test_deploy.py, which already parses YAML.
package mcpdeploy

import (
	"fmt"
	"io"
	"net"
	"net/http"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

const catalogPath = "manifests/catalog.yaml"

// repoRoot is the monorepo root, derived from this file's location
// (projects/home/kubernetes/mcp-gateway).
func repoRoot(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("cannot locate the test file")
	}
	return filepath.Clean(filepath.Join(filepath.Dir(file), "..", "..", "..", ".."))
}

// buildGateway compiles the production command once per test into a temp dir.
func buildGateway(t *testing.T) string {
	t.Helper()
	goBin, err := exec.LookPath("go")
	if err != nil {
		t.Skip("go toolchain not available")
	}
	binary := filepath.Join(t.TempDir(), "mcp-gateway")
	cmd := exec.Command(goBin, "build", "-o", binary, "./solutions/mcp-gateway/cmd/mcp-gateway")
	cmd.Dir = repoRoot(t)
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("go build ./solutions/mcp-gateway/cmd/mcp-gateway: %v\n%s", err, out)
	}
	return binary
}

func freePort(t *testing.T) int {
	t.Helper()
	ln, err := net.Listen("tcp", "127.0.0.1:0")
	if err != nil {
		t.Fatalf("reserve a loopback port: %v", err)
	}
	defer ln.Close()
	return ln.Addr().(*net.TCPAddr).Port
}

// startGateway runs the binary with the given catalog and returns its address.
// The process is stopped when the test finishes.
func startGateway(t *testing.T, binary, catalog string) string {
	t.Helper()
	token := filepath.Join(t.TempDir(), "token")
	if err := os.WriteFile(token, []byte("contract-test-token\n"), 0o600); err != nil {
		t.Fatalf("write token file: %v", err)
	}
	addr := fmt.Sprintf("127.0.0.1:%d", freePort(t))
	cmd := exec.Command(binary)
	cmd.Env = append(os.Environ(),
		"MCP_AUTH_TOKEN_FILE="+token,
		"MCP_INFRA_CATALOG_FILE="+catalog,
		"MCP_GROUPS=infra.read",
		"MCP_HTTP_ADDR="+addr,
		"LOG_LEVEL=error",
	)
	cmd.Stdout = io.Discard
	cmd.Stderr = io.Discard
	if err := cmd.Start(); err != nil {
		t.Fatalf("start gateway: %v", err)
	}
	t.Cleanup(func() {
		_ = cmd.Process.Signal(os.Interrupt)
		done := make(chan struct{})
		go func() { _ = cmd.Wait(); close(done) }()
		select {
		case <-done:
		case <-time.After(5 * time.Second):
			_ = cmd.Process.Kill()
			<-done
		}
	})
	return addr
}

// waitReady polls /readyz until it answers 200 or the deadline passes.
func waitReady(t *testing.T, addr string, timeout time.Duration) error {
	t.Helper()
	client := &http.Client{Timeout: time.Second}
	deadline := time.Now().Add(timeout)
	var last error
	for time.Now().Before(deadline) {
		resp, err := client.Get("http://" + addr + "/readyz")
		if err != nil {
			last = err
		} else {
			body, _ := io.ReadAll(io.LimitReader(resp.Body, 512))
			resp.Body.Close()
			if resp.StatusCode == http.StatusOK && strings.Contains(string(body), "ready") {
				return nil
			}
			last = fmt.Errorf("/readyz returned %d: %s", resp.StatusCode, strings.TrimSpace(string(body)))
		}
		time.Sleep(100 * time.Millisecond)
	}
	if last == nil {
		last = fmt.Errorf("deadline exceeded")
	}
	return last
}

// TestTrackedCatalogLoads is the schema gate for the deployed catalog: startup
// parses and validates it before the listener is created, and readiness only
// turns green once the catalog is loaded.
func TestTrackedCatalogLoads(t *testing.T) {
	binary := buildGateway(t)
	catalog := filepath.Join(repoRoot(t), "projects/home/kubernetes/mcp-gateway", catalogPath)
	addr := startGateway(t, binary, catalog)
	if err := waitReady(t, addr, 20*time.Second); err != nil {
		t.Fatalf("gateway did not become ready with %s: %v", catalogPath, err)
	}
}

// TestInvalidCatalogRefusesToStart proves the check above is not vacuous: the
// same binary must reject a catalog that violates the loader rules (here a
// duplicate id).
func TestInvalidCatalogRefusesToStart(t *testing.T) {
	binary := buildGateway(t)
	broken := filepath.Join(t.TempDir(), "catalog.yaml")
	content := `overview: broken
services:
  - id: duplicate
    aliases: {en: [one]}
    kind: external
    logs: {source: none}
    restart: {allowed: false}
  - id: duplicate
    aliases: {en: [two]}
    kind: external
    logs: {source: none}
    restart: {allowed: false}
`
	if err := os.WriteFile(broken, []byte(content), 0o600); err != nil {
		t.Fatalf("write broken catalog: %v", err)
	}
	token := filepath.Join(t.TempDir(), "token")
	if err := os.WriteFile(token, []byte("contract-test-token\n"), 0o600); err != nil {
		t.Fatalf("write token file: %v", err)
	}
	cmd := exec.Command(binary)
	cmd.Env = append(os.Environ(),
		"MCP_AUTH_TOKEN_FILE="+token,
		"MCP_INFRA_CATALOG_FILE="+broken,
		"MCP_GROUPS=infra.read",
		"MCP_HTTP_ADDR="+fmt.Sprintf("127.0.0.1:%d", freePort(t)),
		"LOG_LEVEL=error",
	)
	out, err := cmd.CombinedOutput()
	if err == nil {
		t.Fatalf("gateway started with a duplicate-id catalog:\n%s", out)
	}
	if !strings.Contains(string(out), "duplicate") {
		t.Fatalf("expected a duplicate-id validation error, got:\n%s", out)
	}
}
