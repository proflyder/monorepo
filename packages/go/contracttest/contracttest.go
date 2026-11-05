// Package contracttest replays golden HTTP contract fixtures against a real
// handler. It is shared by the Go services whose wire contracts must stay
// stable while they are ported to Rust: one JSON file per scenario, the same
// file consumed by the future implementation's test runner.
//
// The fixture format is documented in README.md. Run is the only entry point;
// services provide a handler factory and optional service-specific hooks.
package contracttest

import (
	"bytes"
	"encoding/json"
	"flag"
	"io"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"regexp"
	"strconv"
	"strings"
	"testing"
)

// Update rewrites the response halves of the fixtures instead of comparing
// them. It is a development aid; both Go and Rust implementations should
// replay the committed bytes unchanged.
var Update = flag.Bool("update", false, "rewrite testdata contract fixtures")

// Option customises Run for one service.
type Option func(*runner)

// WithHeaders adds response headers to record and compare. The runner always
// records Content-Type, Connect-Protocol-Version, Grpc-Status and
// Www-Authenticate.
func WithHeaders(names ...string) Option {
	return func(r *runner) {
		for _, name := range names {
			if name != "" {
				r.headers = append(r.headers, name)
			}
		}
	}
}

// WithClient overrides the HTTP client used for the requests. By default Run
// uses the httptest server's client.
func WithClient(client *http.Client) Option {
	return func(r *runner) { r.client = client }
}

// WithBeforeStep runs fn before every request, with the 1-based step number.
// It is the place for a service that needs to advance a fake clock between
// steps.
func WithBeforeStep(fn func(t *testing.T, step int)) Option {
	return func(r *runner) { r.beforeStep = fn }
}

// WithNormalizer rewrites the recorded response before it is written by
// -update. This is where a service turns its own non-deterministic values
// (for example a random UUID or a wall-clock timestamp) into a stable
// placeholder. The generic capture substitution has already run when fn is
// called, so captured values appear as {{name}}.
func WithNormalizer(fn func(scenario string, value any) any) Option {
	return func(r *runner) { r.normalize = fn }
}

// Run replays every *.json fixture in dir. newServer is called once per
// scenario and must return the handler under test built with fake
// dependencies; it may also seed state, because the scenario name is
// available.
//
// dir is the repository-relative fixture directory (for example
// "solutions/scheduler/internal/api/testdata/contract"). Run also tries the
// "testdata/..." suffix, so `go test` from the package directory and Bazel
// from the runfiles workspace root both work.
func Run(t *testing.T, dir string, newServer func(t *testing.T, scenario string) http.Handler, opts ...Option) {
	t.Helper()
	r := &runner{
		headers: []string{
			"Content-Type",
			"Connect-Protocol-Version",
			"Grpc-Status",
			"Www-Authenticate",
		},
	}
	for _, opt := range opts {
		opt(r)
	}
	files := fixtureFiles(t, dir)
	for _, path := range files {
		path := path
		t.Run(filepath.Base(path), func(t *testing.T) {
			r.runFile(t, path, newServer)
		})
	}
}

type runner struct {
	headers    []string
	client     *http.Client
	beforeStep func(t *testing.T, step int)
	normalize  func(scenario string, value any) any
}

// fixtureFiles resolves dir under both `go test` (package cwd) and Bazel
// (runfiles workspace root).
func fixtureFiles(t *testing.T, dir string) []string {
	t.Helper()
	candidates := []string{dir}
	if index := strings.Index(dir, "testdata/"); index >= 0 {
		candidates = append(candidates, dir[index:])
	}
	for _, candidate := range candidates {
		matches, err := filepath.Glob(filepath.Join(candidate, "*.json"))
		if err == nil && len(matches) > 0 {
			return matches
		}
	}
	t.Fatalf("no contract fixtures in %s (tried %s)", dir, strings.Join(candidates, ", "))
	return nil
}

func (r *runner) runFile(t *testing.T, path string, newServer func(t *testing.T, scenario string) http.Handler) {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("read fixture: %v", err)
	}
	var file fixtureFile
	if err := json.Unmarshal(raw, &file); err != nil {
		t.Fatalf("decode fixture: %v", err)
	}

	server := httptest.NewServer(newServer(t, file.Name))
	defer server.Close()
	client := server.Client()
	if r.client != nil {
		client = r.client
	}

	captured := map[string]string{}
	changed := false
	for index := range file.Steps {
		step := &file.Steps[index]
		if r.beforeStep != nil {
			r.beforeStep(t, index+1)
		}
		req, err := buildRequest(server.URL, step.Request, captured)
		if err != nil {
			t.Fatalf("step %d request: %v", index+1, err)
		}
		resp, err := client.Do(req)
		if err != nil {
			t.Fatalf("step %d do: %v", index+1, err)
		}
		body, readErr := io.ReadAll(resp.Body)
		_ = resp.Body.Close()
		if readErr != nil {
			t.Fatalf("step %d read: %v", index+1, readErr)
		}

		actual := decodeBody(body)
		captureHeaders(step, resp.Header, captured)
		if err := captureBody(step, actual, captured); err != nil {
			t.Fatalf("step %d %v", index+1, err)
		}

		if *Update {
			step.Response.Status = resp.StatusCode
			step.Response.Headers = r.selectHeaders(resp.Header)
			step.Response.Body = r.record(actual, captured, file.Name)
			changed = true
			continue
		}

		if resp.StatusCode != step.Response.Status {
			t.Fatalf("step %d status = %d, want %d (body %s)", index+1, resp.StatusCode, step.Response.Status, body)
		}
		for name, want := range step.Response.Headers {
			got := resp.Header.Get(name)
			want = substitute(want, captured)
			if got != want {
				t.Fatalf("step %d header %s = %q, want %q", index+1, name, got, want)
			}
		}
		if step.Response.Body == nil {
			continue
		}
		wantBody, err := resolve(step.Response.Body, captured)
		if err != nil {
			t.Fatalf("step %d expected body: %v", index+1, err)
		}
		if err := compare(wantBody, actual); err != nil {
			prettyGot, _ := json.MarshalIndent(actual, "", "  ")
			prettyWant, _ := json.MarshalIndent(wantBody, "", "  ")
			t.Fatalf("step %d body mismatch: %v\nwant: %s\ngot:  %s", index+1, err, prettyWant, prettyGot)
		}
	}

	if *Update && changed {
		var buffer bytes.Buffer
		encoder := json.NewEncoder(&buffer)
		encoder.SetEscapeHTML(false)
		encoder.SetIndent("", "  ")
		if err := encoder.Encode(file); err != nil {
			t.Fatalf("encode fixture: %v", err)
		}
		if err := os.WriteFile(path, buffer.Bytes(), 0o644); err != nil {
			t.Fatalf("write fixture: %v", err)
		}
	}
}

func (r *runner) record(value any, captured map[string]string, scenario string) any {
	for name, raw := range captured {
		if raw == "" {
			continue
		}
		value = ReplacePattern(value, regexp.MustCompile(regexp.QuoteMeta(raw)), "{{"+name+"}}")
	}
	if r.normalize != nil {
		value = r.normalize(scenario, value)
	}
	return value
}

func (r *runner) selectHeaders(header http.Header) map[string]string {
	out := make(map[string]string)
	for _, name := range r.headers {
		if value := header.Get(name); value != "" {
			out[name] = value
		}
	}
	if len(out) == 0 {
		return nil
	}
	return out
}

// ReplacePattern applies replacement to every string in a decoded JSON value.
// Services use it to turn random identifiers and timestamps into placeholders.
func ReplacePattern(value any, pattern *regexp.Regexp, replacement string) any {
	switch typed := value.(type) {
	case string:
		return pattern.ReplaceAllString(typed, replacement)
	case []any:
		out := make([]any, len(typed))
		for i, item := range typed {
			out[i] = ReplacePattern(item, pattern, replacement)
		}
		return out
	case map[string]any:
		out := make(map[string]any, len(typed))
		for key, item := range typed {
			out[key] = ReplacePattern(item, pattern, replacement)
		}
		return out
	default:
		return value
	}
}

var (
	// UUIDPattern matches a canonical RFC 4122 UUID.
	UUIDPattern = regexp.MustCompile(`[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}`)
	// Base64_32Pattern matches the canonical base64 of 32 bytes (a nonce).
	Base64_32Pattern = regexp.MustCompile(`[A-Za-z0-9+/]{43}=`)
)

func matchPlaceholder(want string, got any) (bool, error) {
	switch want {
	case "<uuid>":
		gotString, ok := got.(string)
		if !ok || !UUIDPattern.MatchString(gotString) {
			return true, &Mismatch{Wanted: want, Got: got}
		}
		return true, nil
	case "<base64-32>":
		gotString, ok := got.(string)
		if !ok || !Base64_32Pattern.MatchString(gotString) {
			return true, &Mismatch{Wanted: want, Got: got}
		}
		return true, nil
	case "<any>":
		if got == nil {
			return true, &Mismatch{Wanted: want, Got: got}
		}
		return true, nil
	}
	return false, nil
}

// Mismatch reports the first structural difference between an expected and an
// actual response body.
type Mismatch struct {
	Wanted any
	Got    any
	Note   string
}

func (m *Mismatch) Error() string {
	note := ""
	if m.Note != "" {
		note = " (" + m.Note + ")"
	}
	return "value mismatch" + note
}

func compare(want, got any) error {
	if wantString, ok := want.(string); ok {
		matched, err := matchPlaceholder(wantString, got)
		if matched {
			return err
		}
	}
	switch typed := want.(type) {
	case map[string]any:
		object, ok := got.(map[string]any)
		if !ok {
			return &Mismatch{Wanted: want, Got: got}
		}
		for key, item := range typed {
			actual, ok := object[key]
			if !ok {
				return &Mismatch{Wanted: want, Got: got, Note: "missing key " + key}
			}
			if err := compare(item, actual); err != nil {
				return err
			}
		}
		for key := range object {
			if _, ok := typed[key]; !ok {
				return &Mismatch{Wanted: want, Got: got, Note: "unexpected key " + key}
			}
		}
		return nil
	case []any:
		items, ok := got.([]any)
		if !ok || len(items) != len(typed) {
			return &Mismatch{Wanted: want, Got: got}
		}
		for i := range typed {
			if err := compare(typed[i], items[i]); err != nil {
				return err
			}
		}
		return nil
	case json.Number:
		number, ok := got.(json.Number)
		if !ok || number.String() != typed.String() {
			return &Mismatch{Wanted: want, Got: got}
		}
		return nil
	case float64:
		// Fixtures are decoded by encoding/json before comparison, so their
		// numbers arrive as float64; responses use json.Number.
		wantNumber := strconv.FormatFloat(typed, 'f', -1, 64)
		if stringify(got) != wantNumber {
			return &Mismatch{Wanted: want, Got: got}
		}
		return nil
	default:
		if want != got {
			return &Mismatch{Wanted: want, Got: got}
		}
		return nil
	}
}

func decodeBody(body []byte) any {
	if len(body) == 0 {
		return nil
	}
	dec := json.NewDecoder(bytes.NewReader(body))
	dec.UseNumber()
	var value any
	if err := dec.Decode(&value); err == nil {
		return value
	}
	// Non-JSON responses (SSE, health text) are compared byte-for-byte.
	return string(body)
}

func captureHeaders(step *fixtureStep, header http.Header, captured map[string]string) {
	for name, source := range step.CaptureHeaders {
		captured[name] = header.Get(source)
	}
}

func captureBody(step *fixtureStep, body any, captured map[string]string) error {
	for name, expr := range step.Capture {
		value, ok := lookupPath(body, expr)
		if !ok {
			return &captureError{name: name, expr: expr}
		}
		captured[name] = stringify(value)
	}
	return nil
}

type captureError struct {
	name string
	expr string
}

func (e *captureError) Error() string {
	return "capture " + e.name + ": path " + e.expr + " not found"
}

func lookupPath(value any, path string) (any, bool) {
	current := value
	for _, part := range strings.Split(path, ".") {
		name, index, hasIndex := splitIndex(part)
		object, ok := current.(map[string]any)
		if !ok {
			return nil, false
		}
		current, ok = object[name]
		if !ok {
			return nil, false
		}
		if hasIndex {
			items, ok := current.([]any)
			if !ok || index < 0 || index >= len(items) {
				return nil, false
			}
			current = items[index]
		}
	}
	return current, true
}

func splitIndex(part string) (string, int, bool) {
	open := strings.Index(part, "[")
	if open < 0 || !strings.HasSuffix(part, "]") {
		return part, 0, false
	}
	index, err := strconv.Atoi(part[open+1 : len(part)-1])
	if err != nil {
		return part, 0, false
	}
	return part[:open], index, true
}

func stringify(value any) string {
	switch typed := value.(type) {
	case nil:
		return ""
	case string:
		return typed
	case json.Number:
		return typed.String()
	default:
		encoded, _ := json.Marshal(typed)
		return string(encoded)
	}
}

var placeholder = regexp.MustCompile(`\{\{([a-zA-Z0-9_]+)\}\}`)

func resolve(value any, captured map[string]string) (any, error) {
	switch typed := value.(type) {
	case string:
		return resolveString(typed, captured)
	case []any:
		out := make([]any, len(typed))
		for i, item := range typed {
			resolved, err := resolve(item, captured)
			if err != nil {
				return nil, err
			}
			out[i] = resolved
		}
		return out, nil
	case map[string]any:
		out := make(map[string]any, len(typed))
		for key, item := range typed {
			resolved, err := resolve(item, captured)
			if err != nil {
				return nil, err
			}
			out[key] = resolved
		}
		return out, nil
	default:
		return value, nil
	}
}

func resolveString(s string, captured map[string]string) (string, error) {
	var missing string
	out := placeholder.ReplaceAllStringFunc(s, func(match string) string {
		name := placeholder.FindStringSubmatch(match)[1]
		value, ok := captured[name]
		if !ok {
			missing = name
			return match
		}
		return value
	})
	if missing != "" {
		return "", &missingCaptureError{name: missing}
	}
	return out, nil
}

func substitute(s string, captured map[string]string) string {
	out, err := resolveString(s, captured)
	if err != nil {
		return s
	}
	return out
}

type missingCaptureError struct{ name string }

func (e *missingCaptureError) Error() string { return "no captured value named " + e.name }

func buildRequest(baseURL string, spec fixtureRequest, captured map[string]string) (*http.Request, error) {
	path, err := resolveString(spec.Path, captured)
	if err != nil {
		return nil, err
	}
	var body io.Reader
	if spec.Body != nil {
		resolved, err := resolve(spec.Body, captured)
		if err != nil {
			return nil, err
		}
		switch value := resolved.(type) {
		case string:
			body = strings.NewReader(value)
		default:
			encoded, err := json.Marshal(value)
			if err != nil {
				return nil, err
			}
			body = bytes.NewReader(encoded)
		}
	}
	req, err := http.NewRequest(spec.Method, baseURL+path, body)
	if err != nil {
		return nil, err
	}
	for name, value := range spec.Headers {
		resolved, err := resolveString(value, captured)
		if err != nil {
			return nil, err
		}
		req.Header.Set(name, resolved)
	}
	return req, nil
}

type fixtureFile struct {
	Name        string        `json:"name"`
	Description string        `json:"description"`
	Steps       []fixtureStep `json:"steps"`
}

type fixtureStep struct {
	Request        fixtureRequest    `json:"request"`
	Response       fixtureResponse   `json:"response"`
	Capture        map[string]string `json:"capture,omitempty"`
	CaptureHeaders map[string]string `json:"captureHeaders,omitempty"`
}

type fixtureRequest struct {
	Method  string            `json:"method"`
	Path    string            `json:"path"`
	Headers map[string]string `json:"headers,omitempty"`
	Body    any               `json:"body,omitempty"`
}

type fixtureResponse struct {
	Status  int               `json:"status"`
	Headers map[string]string `json:"headers,omitempty"`
	Body    any               `json:"body,omitempty"`
}
