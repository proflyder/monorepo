package kubernetes

import (
	"context"
	"encoding/json"
	"io"
	"net/http"
	"net/http/httptest"
	"testing"
	"time"
)

func newTestClient(t *testing.T, handler http.HandlerFunc) (*Client, *httptest.Server) {
	t.Helper()
	srv := httptest.NewServer(handler)
	t.Cleanup(srv.Close)
	client, err := New(Options{BaseURL: srv.URL, Token: "test-token", HTTPClient: srv.Client()})
	if err != nil {
		t.Fatalf("New: %v", err)
	}
	return client, srv
}

func TestGetWorkloadDeployment(t *testing.T) {
	client, _ := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/apis/apps/v1/namespaces/home/deployments/scheduler" {
			t.Errorf("path = %q", r.URL.Path)
		}
		if got := r.Header.Get("Authorization"); got != "Bearer test-token" {
			t.Errorf("authorization = %q", got)
		}
		_, _ = io.WriteString(w, `{
  "metadata": {"name":"scheduler","namespace":"home","generation":3,
    "annotations":{"mcp-gateway/last-restart":"2026-11-01T10:00:00Z"}},
  "spec": {"replicas":1,"selector":{"matchLabels":{"app.kubernetes.io/name":"scheduler"}},
    "template":{"spec":{"containers":[{"name":"scheduler"}]}}},
  "status": {"replicas":1,"readyReplicas":1,"availableReplicas":1,"updatedReplicas":1,"observedGeneration":3}
}`)
	})
	w, err := client.GetWorkload(context.Background(), "home", "Deployment", "scheduler")
	if err != nil {
		t.Fatalf("GetWorkload: %v", err)
	}
	if w.Desired != 1 || w.Ready != 1 || w.Available != 1 || !w.AvailableOK() {
		t.Fatalf("workload = %+v", w)
	}
	if w.RolloutInProgress() {
		t.Fatal("rollout reported in progress")
	}
	if len(w.Containers) != 1 || w.Containers[0] != "scheduler" {
		t.Fatalf("containers = %v", w.Containers)
	}
	if w.Selector["app.kubernetes.io/name"] != "scheduler" {
		t.Fatalf("selector = %v", w.Selector)
	}
	last, ok := LastRestart(w)
	if !ok || !last.Equal(time.Date(2026, 11, 1, 10, 0, 0, 0, time.UTC)) {
		t.Fatalf("LastRestart = %v, %v", last, ok)
	}
}

func TestGetWorkloadDaemonSetAndRollout(t *testing.T) {
	client, _ := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		_, _ = io.WriteString(w, `{
  "metadata": {"name":"alloy","namespace":"monitoring","generation":5},
  "spec": {"selector":{"matchLabels":{"app.kubernetes.io/name":"alloy"}}},
  "status": {"desiredNumberScheduled":3,"numberReady":2,"numberAvailable":2,"updatedNumberScheduled":1,"observedGeneration":4}
}`)
	})
	w, err := client.GetWorkload(context.Background(), "monitoring", "DaemonSet", "alloy")
	if err != nil {
		t.Fatalf("GetWorkload: %v", err)
	}
	if w.Desired != 3 || w.Ready != 2 || w.Updated != 1 {
		t.Fatalf("workload = %+v", w)
	}
	if !w.RolloutInProgress() {
		t.Fatal("rollout not detected")
	}
	if w.AvailableOK() {
		t.Fatal("degraded daemonset reported available")
	}
}

func TestGetWorkloadUnknownKind(t *testing.T) {
	client, _ := newTestClient(t, func(http.ResponseWriter, *http.Request) {})
	if _, err := client.GetWorkload(context.Background(), "home", "Pod", "x"); err == nil {
		t.Fatal("unsupported kind accepted")
	}
}

func TestListPods(t *testing.T) {
	client, _ := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/v1/namespaces/home/pods" {
			t.Errorf("path = %q", r.URL.Path)
		}
		if got := r.URL.Query().Get("labelSelector"); got != "app=hermes" {
			t.Errorf("selector = %q", got)
		}
		_, _ = io.WriteString(w, `{"items":[
 {"metadata":{"name":"hermes-2"},"spec":{"nodeName":"node1","containers":[{"name":"hermes","image":"ghcr.io/example/hermes@sha256:aaaa"},{"name":"voice-proxy","image":"ghcr.io/example/proxy@sha256:bbbb"}]},
  "status":{"phase":"Running","startTime":"2026-11-01T09:00:00Z","containerStatuses":[
    {"name":"hermes","ready":true,"restartCount":2,
     "state":{"waiting":{"reason":""}},
     "lastState":{"terminated":{"finishedAt":"2026-11-01T10:55:00Z"}}},
    {"name":"voice-proxy","ready":false,"restartCount":7,"state":{"waiting":{"reason":"CrashLoopBackOff"}}}]}},
 {"metadata":{"name":"hermes-1"},"status":{"phase":"Pending"}}
]}`)
	})
	pods, err := client.ListPods(context.Background(), "home", map[string]string{"app": "hermes"})
	if err != nil {
		t.Fatalf("ListPods: %v", err)
	}
	if len(pods) != 2 || pods[0].Name != "hermes-1" || pods[1].Name != "hermes-2" {
		t.Fatalf("pods = %+v", pods)
	}
	if pods[0].Ready {
		t.Fatal("pod without container statuses reported ready")
	}
	if pods[1].Ready {
		// hermes-2 has one non-ready container.
		t.Fatal("hermes-2 reported ready with a non-ready container")
	}
	if pods[1].Restarts != 9 {
		t.Fatalf("restarts = %d, want 9", pods[1].Restarts)
	}
	if pods[1].Containers[1].WaitingReason != "CrashLoopBackOff" {
		t.Fatalf("waiting reason = %q", pods[1].Containers[1].WaitingReason)
	}
	if pods[1].Containers[0].LastTerminatedAt.IsZero() {
		t.Fatal("last terminated time missing")
	}
	if pods[1].Containers[0].Image != "ghcr.io/example/hermes@sha256:aaaa" {
		t.Fatalf("image = %q", pods[1].Containers[0].Image)
	}
}

func TestListEvents(t *testing.T) {
	client, _ := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if got := r.URL.Query().Get("fieldSelector"); got != "involvedObject.name=scheduler" {
			t.Errorf("fieldSelector = %q", got)
		}
		_, _ = io.WriteString(w, `{"items":[
 {"type":"Warning","reason":"BackOff","message":"back-off restarting","count":4,"lastTimestamp":"2026-11-01T09:00:00Z"},
 {"type":"Normal","reason":"Pulled","message":"image pulled","count":1,"lastTimestamp":"2026-11-01T10:00:00Z"},
 {"type":"Normal","reason":"Old","message":"very old","count":1,"lastTimestamp":"2026-10-01T10:00:00Z"}
]}`)
	})
	events, err := client.ListEvents(context.Background(), "home", "scheduler", 2)
	if err != nil {
		t.Fatalf("ListEvents: %v", err)
	}
	if len(events) != 2 {
		t.Fatalf("events = %d", len(events))
	}
	if events[0].Reason != "Pulled" || events[1].Reason != "BackOff" {
		t.Fatalf("events not newest-first: %+v", events)
	}
}

func TestPodLogs(t *testing.T) {
	client, _ := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.URL.Path != "/api/v1/namespaces/home/pods/scheduler-1/log" {
			t.Errorf("path = %q", r.URL.Path)
		}
		q := r.URL.Query()
		if q.Get("container") != "scheduler" || q.Get("tailLines") != "50" || q.Get("sinceSeconds") != "900" {
			t.Errorf("query = %v", q)
		}
		_, _ = io.WriteString(w, "line1\nline2\n")
	})
	body, err := client.PodLogs(context.Background(), "home", "scheduler-1", "scheduler", 50, 15*time.Minute)
	if err != nil {
		t.Fatalf("PodLogs: %v", err)
	}
	if body != "line1\nline2\n" {
		t.Fatalf("body = %q", body)
	}
}

func TestPatchRestart(t *testing.T) {
	var gotBody map[string]any
	client, _ := newTestClient(t, func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPatch || r.URL.Path != "/apis/apps/v1/namespaces/home/deployments/scheduler" {
			t.Errorf("request = %s %s", r.Method, r.URL.Path)
		}
		if got := r.Header.Get("Content-Type"); got != "application/strategic-merge-patch+json" {
			t.Errorf("content-type = %q", got)
		}
		if err := json.NewDecoder(r.Body).Decode(&gotBody); err != nil {
			t.Errorf("decode: %v", err)
		}
		_, _ = io.WriteString(w, `{"metadata":{"name":"scheduler"}}`)
	})
	at := time.Date(2026, 11, 1, 12, 0, 0, 0, time.UTC)
	if err := client.PatchRestart(context.Background(), "home", "Deployment", "scheduler", at); err != nil {
		t.Fatalf("PatchRestart: %v", err)
	}
	spec := gotBody["spec"].(map[string]any)
	template := spec["template"].(map[string]any)
	meta := template["metadata"].(map[string]any)
	annotations := meta["annotations"].(map[string]any)
	if annotations["kubectl.kubernetes.io/restartedAt"] != "2026-11-01T12:00:00Z" {
		t.Fatalf("restartedAt = %v", annotations)
	}
	objectMeta := gotBody["metadata"].(map[string]any)
	objectAnnotations := objectMeta["annotations"].(map[string]any)
	if objectAnnotations["mcp-gateway/last-restart"] != "2026-11-01T12:00:00Z" {
		t.Fatalf("last-restart = %v", objectAnnotations)
	}
}

func TestPatchRestartError(t *testing.T) {
	client, _ := newTestClient(t, func(w http.ResponseWriter, _ *http.Request) {
		w.WriteHeader(http.StatusForbidden)
		_, _ = io.WriteString(w, `{"message":"forbidden"}`)
	})
	if err := client.PatchRestart(context.Background(), "home", "Deployment", "scheduler", time.Now()); err == nil {
		t.Fatal("HTTP 403 accepted")
	}
}

func TestLastRestartInvalid(t *testing.T) {
	if _, ok := LastRestart(Workload{Annotations: map[string]string{"mcp-gateway/last-restart": "not-a-time"}}); ok {
		t.Fatal("invalid annotation accepted")
	}
	if _, ok := LastRestart(Workload{}); ok {
		t.Fatal("missing annotation accepted")
	}
}

func TestLabelSelectorSorted(t *testing.T) {
	got := labelSelector(map[string]string{"b": "2", "a": "1"})
	if got != "a=1,b=2" {
		t.Fatalf("selector = %q", got)
	}
}
