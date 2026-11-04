package manifests

import (
	"encoding/base64"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

const externalSecrets = `apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: hermes-environment
  namespace: home-assistant
---
apiVersion: v1
kind: ConfigMap
metadata:
  name: ignored
  namespace: home-assistant
---
apiVersion: external-secrets.io/v1beta1
kind: ExternalSecret
metadata:
  name: scheduler-auth
  namespace: home-assistant
`

func TestExternalSecretNamesInDocumentOrder(t *testing.T) {
	path := filepath.Join(t.TempDir(), "external-secrets.yaml")
	if err := os.WriteFile(path, []byte(externalSecrets), 0o600); err != nil {
		t.Fatal(err)
	}
	names, err := ExternalSecretNames([]string{path})
	if err != nil {
		t.Fatal(err)
	}
	if got := strings.Join(names, ","); got != "home-assistant/hermes-environment,home-assistant/scheduler-auth" {
		t.Fatalf("names = %q", got)
	}
}

func TestExternalSecretNamesRejectsUnnamedSecrets(t *testing.T) {
	path := filepath.Join(t.TempDir(), "bad.yaml")
	os.WriteFile(path, []byte("kind: ExternalSecret\nmetadata:\n  namespace: home-assistant\n"), 0o600)
	if _, err := ExternalSecretNames([]string{path}); err == nil {
		t.Fatal("a nameless ExternalSecret must fail closed")
	}
}

func TestOpaqueSecretReadsQuotedValue(t *testing.T) {
	path := filepath.Join(t.TempDir(), "cloudflare.env")
	if err := os.WriteFile(path, []byte("# comment\nCLOUDFLARE_API_TOKEN=\"token-value\"\n"), 0o600); err != nil {
		t.Fatal(err)
	}
	document, err := OpaqueSecret("cloudflare-api-token", "cert-manager", "api-token", path, "CLOUDFLARE_API_TOKEN")
	if err != nil {
		t.Fatal(err)
	}
	var parsed struct {
		APIVersion string `json:"apiVersion"`
		Kind       string `json:"kind"`
		Type       string `json:"type"`
		Metadata   struct {
			Name      string `json:"name"`
			Namespace string `json:"namespace"`
		} `json:"metadata"`
		Data map[string]string `json:"data"`
	}
	if err := json.Unmarshal(document, &parsed); err != nil {
		t.Fatal(err)
	}
	if parsed.APIVersion != "v1" || parsed.Kind != "Secret" || parsed.Type != "Opaque" {
		t.Fatalf("unexpected document: %s", document)
	}
	if parsed.Metadata.Name != "cloudflare-api-token" || parsed.Metadata.Namespace != "cert-manager" {
		t.Fatalf("unexpected metadata: %s", document)
	}
	decoded, _ := base64.StdEncoding.DecodeString(parsed.Data["api-token"])
	if string(decoded) != "token-value" {
		t.Fatalf("token = %q", decoded)
	}
}

func TestOpaqueSecretFailsClosedWithoutValue(t *testing.T) {
	path := filepath.Join(t.TempDir(), "empty.env")
	os.WriteFile(path, []byte("OTHER=1\n"), 0o600)
	if _, err := OpaqueSecret("name", "ns", "key", path, "CLOUDFLARE_API_TOKEN"); err == nil {
		t.Fatal("a missing token must fail")
	}
}
