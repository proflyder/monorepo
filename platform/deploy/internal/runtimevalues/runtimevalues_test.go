package runtimevalues

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	yaml "go.yaml.in/yaml/v2"
)

const render = `---
apiVersion: v1
kind: PersistentVolume
metadata:
  name: runtime-hermes
spec:
  capacity:
    storage: 10Gi
---
apiVersion: v1
kind: PersistentVolumeClaim
metadata:
  name: runtime-hermes
---
apiVersion: apps/v1
kind: Deployment
metadata:
  name: hermes
`

const private = `nodeName: lab-host
storage:
  hermes:
    size: 10Gi
workloads:
  hermes:
    replicas: 1
    containers:
    - name: gateway
      image: hermes@sha256:aaaa
  runtime-health-exporter:
    replicas: 1
    containers:
    - name: health-exporter
      image: ghcr.io/proflyder/voice-proxy@sha256:old
      command: ["python3", "/metrics/health_exporter.py"]
      volumeMounts:
      - name: metrics
        mountPath: /metrics
    volumes:
    - name: metrics
      configMap:
        name: runtime-health-exporter
`

func writeTemp(t *testing.T, content string) string {
	t.Helper()
	path := filepath.Join(t.TempDir(), "input.yaml")
	if err := os.WriteFile(path, []byte(content), 0o600); err != nil {
		t.Fatal(err)
	}
	return path
}

func TestSplitRenderKeepsOnlyStorage(t *testing.T) {
	output, err := SplitRender(writeTemp(t, render))
	if err != nil {
		t.Fatal(err)
	}
	text := string(output)
	if !strings.Contains(text, "kind: PersistentVolume") || !strings.Contains(text, "kind: PersistentVolumeClaim") {
		t.Fatalf("storage documents missing: %s", text)
	}
	if strings.Contains(text, "kind: Deployment") {
		t.Fatalf("a workload leaked into the storage half: %s", text)
	}
}

func TestWorkloadValuesDropsStorage(t *testing.T) {
	values, err := WorkloadValues(writeTemp(t, private))
	if err != nil {
		t.Fatal(err)
	}
	if _, present := mappingValue(values, "storage"); present {
		t.Fatal("storage survived")
	}
	workloads, _ := mappingValue(values, "workloads")
	if _, present := mappingValue(workloads, "hermes"); !present {
		t.Fatal("workloads were dropped")
	}
}

func TestPinHealthExporterKeepsTheRestOfTheContainer(t *testing.T) {
	values, err := WorkloadValues(writeTemp(t, private))
	if err != nil {
		t.Fatal(err)
	}
	pinned, err := PinHealthExporter(values, "ghcr.io/proflyder/health-exporter@sha256:new", "runtime-health-exporter")
	if err != nil {
		t.Fatal(err)
	}
	workloads, _ := mappingValue(pinned, "workloads")
	exporter, _ := mappingValue(workloads, "runtime-health-exporter")
	if _, present := valueOf(exporter, "volumes"); present {
		t.Fatal("the script volume was not removed")
	}
	containers, _ := valueOf(exporter, "containers")
	container := containers.([]interface{})[0].(yaml.MapSlice)
	if got := stringValue(container, "image"); got != "ghcr.io/proflyder/health-exporter@sha256:new" {
		t.Fatalf("image = %q", got)
	}
	if _, present := valueOf(container, "volumeMounts"); !present {
		t.Fatal("volumeMounts must become an empty list")
	}
	command, _ := valueOf(container, "command")
	if got := command.([]interface{})[0]; got != "/usr/local/bin/health-exporter" {
		t.Fatalf("command = %v", command)
	}
	if _, present := valueOf(container, "name"); !present {
		t.Fatal("the container name was lost")
	}
}
