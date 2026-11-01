// Package kubernetes is a small read-mostly Kubernetes REST client.
//
// The gateway deliberately avoids client-go: it talks to the API server with
// the in-cluster ServiceAccount token and CA, and performs exactly one kind of
// mutation, a rollout restart annotation patch.
package kubernetes

import (
	"bytes"
	"context"
	"crypto/tls"
	"crypto/x509"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"
)

// Defaults for the in-cluster ServiceAccount projected volume.
const (
	DefaultAPIURL    = "https://kubernetes.default.svc"
	DefaultTokenFile = "/var/run/secrets/kubernetes.io/serviceaccount/token"
	DefaultCAFile    = "/var/run/secrets/kubernetes.io/serviceaccount/ca.crt"
)

// Options configures the client.
type Options struct {
	// BaseURL is the API server root. Empty means the in-cluster URL.
	BaseURL string
	// Token is a static bearer token; when empty TokenFile is read per call so
	// projected token rotation keeps working.
	Token string
	// TokenFile defaults to the in-cluster projected token.
	TokenFile string
	// CAFile defaults to the in-cluster CA bundle.
	CAFile string
	// HTTPClient overrides the transport (tests pass an httptest client).
	HTTPClient *http.Client
}

// Client is a Kubernetes REST client.
type Client struct {
	baseURL   string
	token     string
	tokenFile string
	http      *http.Client
}

// New builds a client from options.
func New(opts Options) (*Client, error) {
	baseURL := strings.TrimSuffix(strings.TrimSpace(opts.BaseURL), "/")
	if baseURL == "" {
		baseURL = DefaultAPIURL
	}
	tokenFile := opts.TokenFile
	if tokenFile == "" && opts.Token == "" {
		tokenFile = DefaultTokenFile
	}
	hc := opts.HTTPClient
	if hc == nil {
		transport := http.DefaultTransport.(*http.Transport).Clone()
		caFile := opts.CAFile
		if caFile == "" {
			caFile = DefaultCAFile
		}
		if _, err := os.Stat(caFile); err == nil {
			pem, err := os.ReadFile(caFile)
			if err != nil {
				return nil, fmt.Errorf("read Kubernetes CA: %w", err)
			}
			pool := x509.NewCertPool()
			if !pool.AppendCertsFromPEM(pem) {
				return nil, fmt.Errorf("Kubernetes CA %s contains no certificates", caFile)
			}
			transport.TLSClientConfig = &tls.Config{RootCAs: pool, MinVersion: tls.VersionTLS12}
		}
		hc = &http.Client{Transport: transport, Timeout: 10 * time.Second}
	}
	return &Client{baseURL: baseURL, token: opts.Token, tokenFile: tokenFile, http: hc}, nil
}

// Workload is a normalized Deployment/StatefulSet/DaemonSet view.
type Workload struct {
	Namespace          string            `json:"namespace"`
	Kind               string            `json:"kind"`
	Name               string            `json:"name"`
	Generation         int64             `json:"generation"`
	ObservedGeneration int64             `json:"observedGeneration"`
	Desired            int64             `json:"desired"`
	Updated            int64             `json:"updated"`
	Ready              int64             `json:"ready"`
	Available          int64             `json:"available"`
	Annotations        map[string]string `json:"annotations,omitempty"`
	Containers         []string          `json:"containers,omitempty"`
	Selector           map[string]string `json:"-"`
}

// Available reports whether the workload has all desired replicas available.
func (w Workload) AvailableOK() bool {
	if w.Desired == 0 {
		return true
	}
	return w.Available >= w.Desired && w.Ready >= w.Desired
}

// RolloutInProgress reports whether the controller has not yet observed the
// current generation or not all replicas are updated.
func (w Workload) RolloutInProgress() bool {
	if w.ObservedGeneration != 0 && w.Generation != 0 && w.ObservedGeneration < w.Generation {
		return true
	}
	return w.Updated != 0 && w.Desired != 0 && w.Updated < w.Desired
}

// ContainerStatus is one container's state in a pod.
type ContainerStatus struct {
	Name             string    `json:"name"`
	Image            string    `json:"image,omitempty"`
	Ready            bool      `json:"ready"`
	RestartCount     int32     `json:"restartCount"`
	WaitingReason    string    `json:"waitingReason,omitempty"`
	LastTerminatedAt time.Time `json:"lastTerminatedAt,omitempty"`
}

// Pod is a normalized pod view.
type Pod struct {
	Namespace  string            `json:"namespace"`
	Name       string            `json:"name"`
	Phase      string            `json:"phase"`
	Node       string            `json:"node,omitempty"`
	Ready      bool              `json:"ready"`
	Restarts   int32             `json:"restarts"`
	Containers []ContainerStatus `json:"containers,omitempty"`
	StartTime  time.Time         `json:"startTime,omitempty"`
}

// Event is a normalized Kubernetes event.
type Event struct {
	Type     string    `json:"type"`
	Reason   string    `json:"reason"`
	Message  string    `json:"message"`
	Count    int32     `json:"count"`
	LastSeen time.Time `json:"lastSeen"`
}

// GetWorkload reads one workload object.
func (c *Client) GetWorkload(ctx context.Context, namespace, kind, name string) (Workload, error) {
	resource, err := resourceFor(kind)
	if err != nil {
		return Workload{}, err
	}
	path := fmt.Sprintf("/apis/apps/v1/namespaces/%s/%s/%s", url.PathEscape(namespace), resource, url.PathEscape(name))
	var obj objectJSON
	if err := c.do(ctx, http.MethodGet, path, nil, &obj); err != nil {
		return Workload{}, err
	}
	w := Workload{
		Namespace:          namespace,
		Kind:               kind,
		Name:               name,
		Generation:         obj.Metadata.Generation,
		ObservedGeneration: obj.Status.ObservedGeneration,
		Annotations:        obj.Metadata.Annotations,
	}
	switch kind {
	case "DaemonSet":
		w.Desired = obj.Status.DesiredNumberScheduled
		w.Ready = obj.Status.NumberReady
		w.Available = obj.Status.NumberAvailable
		w.Updated = obj.Status.UpdatedNumberScheduled
	default:
		w.Desired = derefInt32(obj.Spec.Replicas)
		w.Ready = obj.Status.ReadyReplicas
		w.Available = obj.Status.AvailableReplicas
		w.Updated = obj.Status.UpdatedReplicas
	}
	if obj.Spec.Selector != nil {
		w.Selector = obj.Spec.Selector.MatchLabels
	}
	if obj.Spec.Template != nil {
		for _, container := range obj.Spec.Template.Spec.Containers {
			w.Containers = append(w.Containers, container.Name)
		}
	}
	return w, nil
}

// ListPods lists pods in a namespace, optionally filtered by labels.
func (c *Client) ListPods(ctx context.Context, namespace string, labels map[string]string) ([]Pod, error) {
	path := fmt.Sprintf("/api/v1/namespaces/%s/pods", url.PathEscape(namespace))
	if len(labels) > 0 {
		path += "?labelSelector=" + url.QueryEscape(labelSelector(labels))
	}
	var list podListJSON
	if err := c.do(ctx, http.MethodGet, path, nil, &list); err != nil {
		return nil, err
	}
	pods := make([]Pod, 0, len(list.Items))
	for _, item := range list.Items {
		pod := Pod{
			Namespace: namespace,
			Name:      item.Metadata.Name,
			Phase:     item.Status.Phase,
			Node:      item.Spec.NodeName,
		}
		if item.Status.StartTime != nil {
			pod.StartTime = *item.Status.StartTime
		}
		images := make(map[string]string, len(item.Spec.Containers))
		for _, container := range item.Spec.Containers {
			images[container.Name] = container.Image
		}
		pod.Ready = true
		if len(item.Status.ContainerStatuses) == 0 {
			pod.Ready = false
		}
		for _, cs := range item.Status.ContainerStatuses {
			status := ContainerStatus{
				Name:         cs.Name,
				Image:        images[cs.Name],
				Ready:        cs.Ready,
				RestartCount: cs.RestartCount,
				WaitingReason: firstNonEmpty(
					cs.State.Waiting.Reason,
					cs.State.Terminated.Reason,
				),
			}
			if cs.LastState.Terminated.FinishedAt != nil {
				status.LastTerminatedAt = *cs.LastState.Terminated.FinishedAt
			} else if cs.State.Terminated.FinishedAt != nil {
				status.LastTerminatedAt = *cs.State.Terminated.FinishedAt
			}
			if !status.Ready {
				pod.Ready = false
			}
			pod.Restarts += status.RestartCount
			pod.Containers = append(pod.Containers, status)
		}
		pods = append(pods, pod)
	}
	sort.SliceStable(pods, func(i, j int) bool { return pods[i].Name < pods[j].Name })
	return pods, nil
}

// ListEvents returns recent events for one object, newest first, capped at
// limit.
func (c *Client) ListEvents(ctx context.Context, namespace, objectName string, limit int) ([]Event, error) {
	if limit <= 0 {
		limit = 10
	}
	query := url.Values{"fieldSelector": {"involvedObject.name=" + objectName}}
	path := fmt.Sprintf("/api/v1/namespaces/%s/events?%s", url.PathEscape(namespace), query.Encode())
	var list eventListJSON
	if err := c.do(ctx, http.MethodGet, path, nil, &list); err != nil {
		return nil, err
	}
	events := make([]Event, 0, len(list.Items))
	for _, item := range list.Items {
		seen := item.LastTimestamp
		if seen.IsZero() {
			seen = item.EventTime
		}
		if seen.IsZero() {
			seen = item.Metadata.CreationTimestamp
		}
		events = append(events, Event{
			Type:     item.Type,
			Reason:   item.Reason,
			Message:  item.Message,
			Count:    item.Count,
			LastSeen: seen,
		})
	}
	sort.SliceStable(events, func(i, j int) bool { return events[i].LastSeen.After(events[j].LastSeen) })
	if len(events) > limit {
		events = events[:limit]
	}
	return events, nil
}

// PodLogs reads the tail of one container's log.
func (c *Client) PodLogs(ctx context.Context, namespace, pod, container string, tailLines int, since time.Duration) (string, error) {
	if tailLines <= 0 {
		tailLines = 200
	}
	query := url.Values{"tailLines": {strconv.Itoa(tailLines)}}
	if container != "" {
		query.Set("container", container)
	}
	if since > 0 {
		query.Set("sinceSeconds", strconv.Itoa(int(since.Seconds())))
	}
	path := fmt.Sprintf("/api/v1/namespaces/%s/pods/%s/log?%s",
		url.PathEscape(namespace), url.PathEscape(pod), query.Encode())
	var body bytes.Buffer
	if err := c.doRaw(ctx, http.MethodGet, path, nil, &body); err != nil {
		return "", err
	}
	return body.String(), nil
}

// PatchRestart performs the only mutation the gateway supports: a strategic
// merge patch of the pod template's restartedAt annotation plus a cooldown
// marker on the object itself.
func (c *Client) PatchRestart(ctx context.Context, namespace, kind, name string, at time.Time) error {
	resource, err := resourceFor(kind)
	if err != nil {
		return err
	}
	stamp := at.UTC().Format(time.RFC3339)
	patch := map[string]any{
		"metadata": map[string]any{
			"annotations": map[string]string{
				"mcp-gateway/last-restart": stamp,
			},
		},
		"spec": map[string]any{
			"template": map[string]any{
				"metadata": map[string]any{
					"annotations": map[string]string{
						"kubectl.kubernetes.io/restartedAt": stamp,
					},
				},
			},
		},
	}
	payload, err := json.Marshal(patch)
	if err != nil {
		return err
	}
	path := fmt.Sprintf("/apis/apps/v1/namespaces/%s/%s/%s", url.PathEscape(namespace), resource, url.PathEscape(name))
	req, err := c.request(ctx, http.MethodPatch, path, bytes.NewReader(payload))
	if err != nil {
		return err
	}
	req.Header.Set("Content-Type", "application/strategic-merge-patch+json")
	resp, err := c.http.Do(req)
	if err != nil {
		return fmt.Errorf("kubernetes patch: %w", err)
	}
	defer resp.Body.Close()
	_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 1<<20))
	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("kubernetes patch %s/%s returned HTTP %d", kind, name, resp.StatusCode)
	}
	return nil
}

// LastRestart reads the gateway cooldown annotation from a workload.
func LastRestart(w Workload) (time.Time, bool) {
	raw := strings.TrimSpace(w.Annotations["mcp-gateway/last-restart"])
	if raw == "" {
		return time.Time{}, false
	}
	parsed, err := time.Parse(time.RFC3339, raw)
	if err != nil {
		return time.Time{}, false
	}
	return parsed, true
}

func (c *Client) request(ctx context.Context, method, path string, body io.Reader) (*http.Request, error) {
	req, err := http.NewRequestWithContext(ctx, method, c.baseURL+path, body)
	if err != nil {
		return nil, err
	}
	token := c.token
	if token == "" && c.tokenFile != "" {
		data, err := os.ReadFile(filepath.Clean(c.tokenFile))
		if err != nil {
			return nil, fmt.Errorf("read Kubernetes token: %w", err)
		}
		token = strings.TrimSpace(string(data))
	}
	if token != "" {
		req.Header.Set("Authorization", "Bearer "+token)
	}
	req.Header.Set("Accept", "application/json")
	return req, nil
}

func (c *Client) do(ctx context.Context, method, path string, body io.Reader, out any) error {
	var buf bytes.Buffer
	if err := c.doRaw(ctx, method, path, body, &buf); err != nil {
		return err
	}
	if err := json.Unmarshal(buf.Bytes(), out); err != nil {
		return fmt.Errorf("kubernetes response is not JSON: %w", err)
	}
	return nil
}

func (c *Client) doRaw(ctx context.Context, method, path string, body io.Reader, out io.Writer) error {
	req, err := c.request(ctx, method, path, body)
	if err != nil {
		return err
	}
	resp, err := c.http.Do(req)
	if err != nil {
		return fmt.Errorf("kubernetes request: %w", err)
	}
	defer resp.Body.Close()
	if resp.StatusCode != http.StatusOK {
		_, _ = io.Copy(io.Discard, io.LimitReader(resp.Body, 1<<20))
		return fmt.Errorf("kubernetes %s %s returned HTTP %d", method, path, resp.StatusCode)
	}
	if _, err := io.Copy(out, io.LimitReader(resp.Body, 4<<20)); err != nil {
		return fmt.Errorf("kubernetes response: %w", err)
	}
	return nil
}

func resourceFor(kind string) (string, error) {
	switch kind {
	case "Deployment":
		return "deployments", nil
	case "StatefulSet":
		return "statefulsets", nil
	case "DaemonSet":
		return "daemonsets", nil
	default:
		return "", fmt.Errorf("unsupported workload kind %q", kind)
	}
}

func labelSelector(labels map[string]string) string {
	keys := make([]string, 0, len(labels))
	for key := range labels {
		keys = append(keys, key)
	}
	sort.Strings(keys)
	parts := make([]string, 0, len(keys))
	for _, key := range keys {
		parts = append(parts, key+"="+labels[key])
	}
	return strings.Join(parts, ",")
}

func derefInt32(v *int32) int64 {
	if v == nil {
		return 0
	}
	return int64(*v)
}

func firstNonEmpty(values ...string) string {
	for _, value := range values {
		if value != "" {
			return value
		}
	}
	return ""
}

// Wire structures: only the fields the gateway reads.

type objectJSON struct {
	Metadata struct {
		Name        string            `json:"name"`
		Namespace   string            `json:"namespace"`
		Generation  int64             `json:"generation"`
		Annotations map[string]string `json:"annotations"`
	} `json:"metadata"`
	Spec struct {
		Replicas *int32 `json:"replicas"`
		Selector *struct {
			MatchLabels map[string]string `json:"matchLabels"`
		} `json:"selector"`
		Template *struct {
			Spec struct {
				Containers []struct {
					Name string `json:"name"`
				} `json:"containers"`
			} `json:"spec"`
		} `json:"template"`
	} `json:"spec"`
	Status struct {
		Replicas               int64 `json:"replicas"`
		ReadyReplicas          int64 `json:"readyReplicas"`
		AvailableReplicas      int64 `json:"availableReplicas"`
		UpdatedReplicas        int64 `json:"updatedReplicas"`
		ObservedGeneration     int64 `json:"observedGeneration"`
		DesiredNumberScheduled int64 `json:"desiredNumberScheduled"`
		NumberReady            int64 `json:"numberReady"`
		NumberAvailable        int64 `json:"numberAvailable"`
		UpdatedNumberScheduled int64 `json:"updatedNumberScheduled"`
	} `json:"status"`
}

type podListJSON struct {
	Items []struct {
		Metadata struct {
			Name string `json:"name"`
		} `json:"metadata"`
		Spec struct {
			NodeName   string `json:"nodeName"`
			Containers []struct {
				Name  string `json:"name"`
				Image string `json:"image"`
			} `json:"containers"`
		} `json:"spec"`
		Status struct {
			Phase             string     `json:"phase"`
			StartTime         *time.Time `json:"startTime"`
			ContainerStatuses []struct {
				Name         string `json:"name"`
				Ready        bool   `json:"ready"`
				RestartCount int32  `json:"restartCount"`
				State        struct {
					Waiting struct {
						Reason string `json:"reason"`
					} `json:"waiting"`
					Terminated struct {
						Reason     string     `json:"reason"`
						FinishedAt *time.Time `json:"finishedAt"`
					} `json:"terminated"`
				} `json:"state"`
				LastState struct {
					Terminated struct {
						FinishedAt *time.Time `json:"finishedAt"`
					} `json:"terminated"`
				} `json:"lastState"`
			} `json:"containerStatuses"`
		} `json:"status"`
	} `json:"items"`
}

type eventListJSON struct {
	Items []struct {
		Metadata struct {
			CreationTimestamp time.Time `json:"creationTimestamp"`
		} `json:"metadata"`
		Type          string    `json:"type"`
		Reason        string    `json:"reason"`
		Message       string    `json:"message"`
		Count         int32     `json:"count"`
		LastTimestamp time.Time `json:"lastTimestamp"`
		EventTime     time.Time `json:"eventTime"`
	} `json:"items"`
}
