// Package prepull pulls a unit's new images onto the node before the deploy
// applies anything.
//
// A deploy that rolls many freshly pushed images used to spend its rollout
// timeout waiting for the kubelet to download them (run 42188744175: 9 images,
// 3–5 minutes of pulls, then a rollback). Prepull creates one short-lived Pod
// per image, waits until the kubelet has the image, and deletes the Pods, so
// the rollout that follows only measures the start of the new version.
//
// The Pods never need to run: distroless images have no shell, so the
// container command is a path that does not exist. What matters is that the
// kubelet pulled the image, which shows as an imageID or as any container
// state past the pull phase.
package prepull

import (
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"sort"
	"strings"
	"time"

	"github.com/proflyder/monorepo/platform/deploy/internal/kube"
)

// LabelKey marks every Pod one prepull run creates; the value is the run id.
const LabelKey = "home-deploy/prepull"

// Options configures one prepull run.
type Options struct {
	Namespace string
	Images    []string
	Timeout   time.Duration
	// Poll is the interval between status reads (default 2s).
	Poll time.Duration
	// RunID names the Pods; random when empty.
	RunID string
	// Now and Sleep are injectable for tests.
	Now   func() time.Time
	Sleep func(time.Duration)
	// Log receives one line per finished image (optional).
	Log func(string)
}

// pullPhaseReasons are the waiting reasons of a container whose image is not
// on the node yet.
var pullPhaseReasons = map[string]bool{
	"":                  true,
	"ContainerCreating": true,
	"PodInitializing":   true,
	"ErrImagePull":      true,
	"ImagePullBackOff":  true,
}

// Run pulls every image and always deletes the Pods it created.
func Run(kubectl kube.Runner, opts Options) (err error) {
	images := unique(opts.Images)
	if len(images) == 0 {
		return nil
	}
	if opts.Namespace == "" {
		return errors.New("prepull: namespace is required")
	}
	if opts.Timeout <= 0 {
		return errors.New("prepull: timeout must be positive")
	}
	if opts.Poll <= 0 {
		opts.Poll = 2 * time.Second
	}
	if opts.Now == nil {
		opts.Now = time.Now
	}
	if opts.Sleep == nil {
		opts.Sleep = time.Sleep
	}
	if opts.RunID == "" {
		if opts.RunID, err = randomID(); err != nil {
			return err
		}
	}
	defer func() {
		if cleanupErr := Cleanup(kubectl, opts.Namespace, opts.RunID); cleanupErr != nil && err == nil {
			err = cleanupErr
		}
	}()

	pods := make(map[string]string, len(images)) // pod name -> image
	for i, image := range images {
		name := fmt.Sprintf("prepull-%s-%d", opts.RunID, i)
		if err := create(kubectl, opts.Namespace, name, image, opts.RunID); err != nil {
			return err
		}
		pods[name] = image
	}

	deadline := opts.Now().Add(opts.Timeout)
	pending := make(map[string]string, len(pods)) // pod -> last waiting reason
	for name := range pods {
		pending[name] = ""
	}
	for {
		for name := range pending {
			pulled, reason, err := status(kubectl, opts.Namespace, name)
			if err != nil {
				return err
			}
			if pulled {
				delete(pending, name)
				if opts.Log != nil {
					opts.Log("prepull: " + pods[name] + " is on the node")
				}
				continue
			}
			pending[name] = reason
		}
		if len(pending) == 0 {
			return nil
		}
		if !opts.Now().Before(deadline) {
			return timeoutError(pods, pending, opts.Timeout)
		}
		opts.Sleep(opts.Poll)
	}
}

// Cleanup deletes every Pod of one run; missing Pods are not an error.
func Cleanup(kubectl kube.Runner, namespace, runID string) error {
	result := kubectl([]string{
		"-n", namespace, "delete", "pod",
		"-l", LabelKey + "=" + runID,
		"--ignore-not-found", "--wait=false",
	})
	if result.Code != 0 {
		return fmt.Errorf("prepull: delete pods of run %s: %s", runID, strings.TrimSpace(result.Stderr))
	}
	return nil
}

func create(kubectl kube.Runner, namespace, name, image, runID string) error {
	overrides, err := json.Marshal(podOverrides(name, image))
	if err != nil {
		return err
	}
	result := kubectl([]string{
		"-n", namespace, "run", name,
		"--image=" + image,
		"--restart=Never",
		"--image-pull-policy=IfNotPresent",
		"--labels=" + LabelKey + "=" + runID,
		"--overrides=" + string(overrides),
	})
	if result.Code != 0 {
		return fmt.Errorf("prepull: create pod for %s: %s", image, strings.TrimSpace(result.Stderr))
	}
	return nil
}

// podOverrides is the restricted Pod spec: the same security context as the
// services, tiny requests, and a command that never exists (pull only). The
// container keeps the Pod's name: `kubectl run --overrides` merges containers by
// name, so a different name would add a second container.
func podOverrides(name, image string) map[string]any {
	return map[string]any{
		"apiVersion": "v1",
		"spec": map[string]any{
			"restartPolicy":                 "Never",
			"automountServiceAccountToken":  false,
			"terminationGracePeriodSeconds": 0,
			"securityContext": map[string]any{
				"runAsNonRoot":   true,
				"runAsUser":      65534,
				"runAsGroup":     65534,
				"seccompProfile": map[string]any{"type": "RuntimeDefault"},
			},
			"containers": []any{map[string]any{
				"name":            name,
				"image":           image,
				"imagePullPolicy": "IfNotPresent",
				"command":         []string{"/home-deploy-prepull-noop"},
				"resources": map[string]any{
					"requests": map[string]string{"cpu": "1m", "memory": "4Mi"},
					"limits":   map[string]string{"memory": "16Mi"},
				},
				"securityContext": map[string]any{
					"allowPrivilegeEscalation": false,
					"readOnlyRootFilesystem":   true,
					"capabilities":             map[string]any{"drop": []string{"ALL"}},
				},
			}},
		},
	}
}

type containerState struct {
	Waiting *struct {
		Reason  string `json:"reason"`
		Message string `json:"message"`
	} `json:"waiting"`
	Running    *struct{} `json:"running"`
	Terminated *struct{} `json:"terminated"`
}

type podStatus struct {
	Status struct {
		ContainerStatuses []struct {
			ImageID string         `json:"imageID"`
			State   containerState `json:"state"`
		} `json:"containerStatuses"`
	} `json:"status"`
}

// status reports whether the image of the Pod is on the node, plus the last
// waiting reason (with message) for the timeout error.
func status(kubectl kube.Runner, namespace, name string) (bool, string, error) {
	result := kubectl([]string{"-n", namespace, "get", "pod", name, "-o", "json"})
	if result.Code != 0 {
		return false, "", fmt.Errorf("prepull: read pod %s: %s", name, strings.TrimSpace(result.Stderr))
	}
	var pod podStatus
	if err := json.Unmarshal([]byte(result.Stdout), &pod); err != nil {
		return false, "", fmt.Errorf("prepull: decode pod %s: %w", name, err)
	}
	if len(pod.Status.ContainerStatuses) == 0 {
		return false, "", nil
	}
	container := pod.Status.ContainerStatuses[0]
	if container.ImageID != "" || container.State.Running != nil || container.State.Terminated != nil {
		return true, "", nil
	}
	if waiting := container.State.Waiting; waiting != nil {
		if !pullPhaseReasons[waiting.Reason] {
			// CreateContainerError, RunContainerError, …: the image is there.
			return true, "", nil
		}
		reason := waiting.Reason
		if waiting.Message != "" {
			reason += ": " + waiting.Message
		}
		return false, reason, nil
	}
	return false, "", nil
}

func timeoutError(pods, pending map[string]string, timeout time.Duration) error {
	lines := make([]string, 0, len(pending))
	for name, reason := range pending {
		if reason == "" {
			reason = "still pulling"
		}
		lines = append(lines, pods[name]+" ("+reason+")")
	}
	sort.Strings(lines)
	return fmt.Errorf("prepull: images not on the node after %s: %s", timeout, strings.Join(lines, "; "))
}

func unique(images []string) []string {
	seen := make(map[string]bool, len(images))
	out := make([]string, 0, len(images))
	for _, image := range images {
		image = strings.TrimSpace(image)
		if image == "" || seen[image] {
			continue
		}
		seen[image] = true
		out = append(out, image)
	}
	return out
}

func randomID() (string, error) {
	buf := make([]byte, 4)
	if _, err := rand.Read(buf); err != nil {
		return "", fmt.Errorf("prepull: run id: %w", err)
	}
	return hex.EncodeToString(buf), nil
}
