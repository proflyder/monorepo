package oci

import (
	"archive/tar"
	"bytes"
	"compress/gzip"
	"io"
	"os"
	"path/filepath"
	"runtime"
	"strings"
	"testing"
	"time"
)

// goldenManifestDigest is the digest the Python helper printed for the committed
// fixture (platform/deploy/testdata/oci/layout) when the fixtures were
// captured. Changing the fixture means regenerating and updating this value.
const goldenManifestDigest = "sha256:8883b693cf69f68bc4762c2764d610df8e6dd8388f6b6e536c0d90a50bf82880"

func repoRoot(t *testing.T) string {
	t.Helper()
	_, file, _, ok := runtime.Caller(0)
	if !ok {
		t.Fatal("cannot resolve the test file path")
	}
	directory := filepath.Dir(file)
	for {
		if _, err := os.Stat(filepath.Join(directory, "go.mod")); err == nil {
			return directory
		}
		parent := filepath.Dir(directory)
		if parent == directory {
			t.Fatal("cannot find the module root above the test file")
		}
		directory = parent
	}
}

func fixture(t *testing.T, relative string) string {
	t.Helper()
	if sourceRoot := os.Getenv("TEST_SRCDIR"); sourceRoot != "" {
		// Bzlmod names the main repository `_main` (TEST_WORKSPACE); older
		// layouts use the module name. Try both before falling back.
		for _, workspace := range []string{os.Getenv("TEST_WORKSPACE"), "_main", "yandex_station_lab"} {
			if workspace == "" {
				continue
			}
			candidate := filepath.Join(sourceRoot, workspace, relative)
			if _, err := os.Stat(candidate); err == nil {
				return candidate
			}
		}
		t.Fatalf("runfiles do not contain %s; add it to the go_test data", relative)
	}
	return filepath.Join(repoRoot(t), relative)
}

func TestDigest(t *testing.T) {
	digest, err := Digest(fixture(t, "platform/deploy/testdata/oci/layout"))
	if err != nil {
		t.Fatalf("digest: %v", err)
	}
	if digest != goldenManifestDigest {
		t.Fatalf("digest = %q, want %q", digest, goldenManifestDigest)
	}
}

func TestPackMatchesPythonBytes(t *testing.T) {
	layout := fixture(t, "platform/deploy/testdata/oci/layout")
	output := filepath.Join(t.TempDir(), "image.tar")
	digest, err := Pack(layout, "ghcr.io/proflyder/golden:test", output)
	if err != nil {
		t.Fatalf("pack: %v", err)
	}
	if digest != goldenManifestDigest {
		t.Fatalf("digest = %q, want %q", digest, goldenManifestDigest)
	}
	assertSameBytes(t, fixture(t, "platform/deploy/testdata/oci/pack.tar"), output)

	// The archive must stay `ctr images import` compatible: the layout files
	// first, then the sorted blobs, each with mode 0644 and mtime 0.
	entries := readTar(t, output)
	names := make([]string, 0, len(entries))
	for _, entry := range entries {
		names = append(names, entry.Name)
	}
	wantPrefix := []string{"oci-layout", "index.json"}
	for index, name := range wantPrefix {
		if names[index] != name {
			t.Fatalf("entry %d = %q, want %q (order %v)", index, names[index], name, names)
		}
	}
	if !strings.HasPrefix(names[2], "blobs/sha256/") {
		t.Fatalf("entry 2 = %q, want a blob", names[2])
	}
	for _, entry := range entries {
		if entry.Mode != 0o644 {
			t.Errorf("%s: mode = %o, want 644", entry.Name, entry.Mode)
		}
		if !entry.ModTime.Equal(time.Unix(0, 0)) {
			t.Errorf("%s: mtime = %v, want the epoch (Python writes mtime=0)", entry.Name, entry.ModTime)
		}
		if entry.Format != tar.FormatUSTAR {
			t.Errorf("%s: format = %v, want USTAR", entry.Name, entry.Format)
		}
	}
	index := string(entries[1].Data)
	if !strings.Contains(index, `"org.opencontainers.image.ref.name": "ghcr.io/proflyder/golden:test"`) {
		t.Fatalf("index.json has no ref annotation:\n%s", index)
	}
	if strings.HasSuffix(index, "\n") {
		t.Fatalf("index.json must not end with a newline (Python json.dumps): %q", index)
	}
}

func TestDockerArchiveMatchesPythonBytes(t *testing.T) {
	layout := fixture(t, "platform/deploy/testdata/oci/layout")
	output := filepath.Join(t.TempDir(), "image-docker.tar")
	digest, err := DockerArchive(layout, "ghcr.io/proflyder/golden:test", output)
	if err != nil {
		t.Fatalf("docker archive: %v", err)
	}
	if digest != goldenManifestDigest {
		t.Fatalf("digest = %q, want %q", digest, goldenManifestDigest)
	}
	assertSameBytes(t, fixture(t, "platform/deploy/testdata/oci/docker.tar"), output)

	entries := readTar(t, output)
	if got := entries[len(entries)-1].Name; got != "manifest.json" {
		t.Fatalf("last entry = %q, want manifest.json", got)
	}
	manifestRaw := string(entries[len(entries)-1].Data)
	if !strings.Contains(manifestRaw, `"RepoTags": ["ghcr.io/proflyder/golden:test"]`) {
		t.Fatalf("manifest.json = %s", manifestRaw)
	}

	// The layer entry is the *uncompressed* tar of the OCI blob.
	layer := entries[1]
	blobDigest := strings.TrimSuffix(layer.Name, "/layer.tar")
	blob, err := os.ReadFile(filepath.Join(layout, "blobs", "sha256", blobDigest))
	if err != nil {
		t.Fatalf("blob %s: %v", blobDigest, err)
	}
	reader, err := gzip.NewReader(bytes.NewReader(blob))
	if err != nil {
		t.Fatalf("gzip: %v", err)
	}
	decompressed, err := io.ReadAll(reader)
	if err != nil {
		t.Fatalf("gunzip: %v", err)
	}
	if !bytes.Equal(decompressed, layer.Data) {
		t.Fatal("docker archive layer is not the decompressed OCI blob")
	}
}

func TestErrors(t *testing.T) {
	layout := t.TempDir()
	// A digest with an unsupported algorithm is refused.
	if err := os.WriteFile(filepath.Join(layout, "index.json"), []byte(
		`{"manifests":[{"digest":"sha512:abc"}]}`), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := Digest(layout); err == nil || !strings.Contains(err.Error(), "unsupported digest: sha512:abc") {
		t.Fatalf("unexpected error: %v", err)
	}

	// Zero or multiple manifests are refused with the Python message.
	empty := t.TempDir()
	if err := os.WriteFile(filepath.Join(empty, "index.json"), []byte(`{"manifests":[]}`), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := Digest(empty); err == nil || !strings.Contains(err.Error(), "expected one image manifest, found 0") {
		t.Fatalf("unexpected error: %v", err)
	}
	two := t.TempDir()
	if err := os.WriteFile(filepath.Join(two, "index.json"), []byte(
		`{"manifests":[{"digest":"sha256:a"},{"digest":"sha256:b"}]}`), 0o600); err != nil {
		t.Fatal(err)
	}
	if _, err := Digest(two); err == nil || !strings.Contains(err.Error(), "expected one image manifest, found 2") {
		t.Fatalf("unexpected error: %v", err)
	}
}

func assertSameBytes(t *testing.T, wantPath, gotPath string) {
	t.Helper()
	want, err := os.ReadFile(wantPath)
	if err != nil {
		t.Fatalf("read golden %s: %v", wantPath, err)
	}
	got, err := os.ReadFile(gotPath)
	if err != nil {
		t.Fatalf("read output %s: %v", gotPath, err)
	}
	if !bytes.Equal(want, got) {
		t.Fatalf("%s differs from the Python golden %s (%d vs %d bytes)", gotPath, wantPath, len(got), len(want))
	}
}

type tarEntry struct {
	Name    string
	Mode    int64
	ModTime time.Time
	Format  tar.Format
	Data    []byte
}

func readTar(t *testing.T, path string) []tarEntry {
	t.Helper()
	raw, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	reader := tar.NewReader(bytes.NewReader(raw))
	entries := make([]tarEntry, 0)
	for {
		header, err := reader.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			t.Fatalf("read tar: %v", err)
		}
		data, err := io.ReadAll(reader)
		if err != nil {
			t.Fatalf("read entry %s: %v", header.Name, err)
		}
		entries = append(entries, tarEntry{
			Name:    header.Name,
			Mode:    header.Mode,
			ModTime: header.ModTime,
			Format:  header.Format,
			Data:    data,
		})
	}
	return entries
}
