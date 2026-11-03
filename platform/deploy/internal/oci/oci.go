// Package oci inspects and packages the OCI image layouts produced by
// `rules_oci`.
//
// It is a port of `platform/ci/oci_image.py`. `rules_oci` writes a plain OCI
// image layout directory (`index.json` + `blobs/`); `oci_load` is unusable when
// the image platform (linux/amd64) differs from the build host (e.g. macOS)
// because it selects the linux `tar` toolchain. These operations are therefore
// platform independent:
//
//	Digest  print the manifest digest of the (single) image in a layout
//	Pack    write a standard OCI archive with a repo tag annotation, suitable
//	        for `ctr images import` on the K3s node
//	Docker  write a docker-archive (`docker load`) from the same layout, for
//	        local smoke tests on a workstation
//
// All three are byte-preserving: they copy blobs, they never rewrite them, so
// the digest printed by Digest is exactly the digest a registry stores after the
// archive is pushed. The archive bytes are identical to the Python helper's:
// PAX-capable USTAR entries with mode 0644, mtime 0 and the layout's blobs in
// sorted order.
package oci

import (
	"bytes"
	"compress/gzip"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// indexFile is the OCI image layout entry point.
const indexFile = "index.json"

// refAnnotation is the standard annotation `ctr images import` uses as the
// image reference.
const refAnnotation = "org.opencontainers.image.ref.name"

// Layout is one OCI image layout directory.
type Layout struct {
	// Dir is the layout directory.
	Dir string
}

// Manifest is the single image manifest referenced by the layout index.
type Manifest struct {
	// Digest is the manifest blob digest as written in the index.
	Digest string
	// Value is the parsed manifest blob.
	Value *jsonValue
	// Object is the same manifest as an ordered object.
	Object *orderedObject
	// Index is the parsed layout index.
	Index *jsonValue
	// IndexObject is the same index as an ordered object.
	IndexObject *orderedObject
}

// ReadIndex parses `<dir>/index.json` with key order preserved.
func ReadIndex(dir string) (*jsonValue, *orderedObject, error) {
	raw, err := os.ReadFile(filepath.Join(dir, indexFile))
	if err != nil {
		return nil, nil, err
	}
	value, err := decodeJSON(raw)
	if err != nil {
		return nil, nil, err
	}
	if value.kind != kindObject {
		return nil, nil, fmt.Errorf("%s: index is not a JSON object", dir)
	}
	return value, value.obj, nil
}

// blobPath resolves a `sha256:<hex>` digest to its path in the layout.
func blobPath(dir, digest string) (string, error) {
	algorithm, value, found := strings.Cut(digest, ":")
	if !found || algorithm != "sha256" || value == "" {
		return "", fmt.Errorf("unsupported digest: %s", digest)
	}
	return filepath.Join(dir, "blobs", algorithm, value), nil
}

// ReadManifest loads the only image manifest referenced by the layout index.
func ReadManifest(dir string) (*Manifest, error) {
	index, indexObject, err := ReadIndex(dir)
	if err != nil {
		return nil, err
	}
	manifests := jsonArrayOf(indexObject, "manifests")
	if len(manifests) != 1 {
		return nil, fmt.Errorf("%s: expected one image manifest, found %d", dir, len(manifests))
	}
	if manifests[0].kind != kindObject {
		return nil, fmt.Errorf("%s: image manifest is not a JSON object", dir)
	}
	digest, ok := fieldString(manifests[0].obj, "digest")
	if !ok {
		return nil, fmt.Errorf("%s: image manifest has no digest", dir)
	}
	path, err := blobPath(dir, digest)
	if err != nil {
		return nil, err
	}
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	value, err := decodeJSON(raw)
	if err != nil {
		return nil, err
	}
	if value.kind != kindObject {
		return nil, fmt.Errorf("%s: manifest blob is not a JSON object", path)
	}
	return &Manifest{
		Digest:      digest,
		Value:       value,
		Object:      value.obj,
		Index:       index,
		IndexObject: indexObject,
	}, nil
}

// Digest returns the manifest digest of the (single) image in the layout.
func Digest(dir string) (string, error) {
	manifest, err := ReadManifest(dir)
	if err != nil {
		return "", err
	}
	return manifest.Digest, nil
}

// Pack writes a standard OCI archive to output, annotating the image with ref,
// and returns the manifest digest.
func Pack(dir, ref, output string) (string, error) {
	manifest, err := ReadManifest(dir)
	if err != nil {
		return "", err
	}

	// Add the reference annotation to the index, preserving every other field
	// and the key order.
	manifests := jsonArrayOf(manifest.IndexObject, "manifests")
	entry := manifests[0].obj
	annotations, found := entry.get("annotations")
	if !found || annotations.kind != kindObject {
		annotations = &jsonValue{kind: kindObject, obj: newJSONObject()}
		entry.set("annotations", annotations)
	}
	annotations.obj.set(refAnnotation, &jsonValue{kind: kindString, str: ref})

	blobs, err := listBlobs(dir)
	if err != nil {
		return "", err
	}

	archive, err := newArchive(output)
	if err != nil {
		return "", err
	}
	closeWith := archive.CloseWith

	layoutFile, err := os.ReadFile(filepath.Join(dir, "oci-layout"))
	if err != nil {
		return "", closeWith(err)
	}
	if err := archive.AddFile("oci-layout", layoutFile); err != nil {
		return "", closeWith(err)
	}
	if err := archive.AddFile(indexFile, dumpJSONIndent(manifest.Index)); err != nil {
		return "", closeWith(err)
	}
	for _, relative := range blobs {
		data, err := os.ReadFile(filepath.Join(dir, filepath.FromSlash(relative)))
		if err != nil {
			return "", closeWith(err)
		}
		if err := archive.AddFile(relative, data); err != nil {
			return "", closeWith(err)
		}
	}
	if err := closeWith(nil); err != nil {
		return "", err
	}
	return manifest.Digest, nil
}

// DockerArchive writes a docker `load` archive to output: uncompressed layers
// plus a `manifest.json`, and returns the manifest digest.
func DockerArchive(dir, ref, output string) (string, error) {
	manifest, err := ReadManifest(dir)
	if err != nil {
		return "", err
	}

	config, found := manifest.Object.get("config")
	if !found || config.kind != kindObject {
		return "", fmt.Errorf("%s: manifest has no config", dir)
	}
	configDigest, ok := fieldString(config.obj, "digest")
	if !ok {
		return "", fmt.Errorf("%s: manifest config has no digest", dir)
	}
	configName := strings.TrimPrefix(configDigest, "sha256:") + ".json"
	configPath, err := blobPath(dir, configDigest)
	if err != nil {
		return "", err
	}
	configBytes, err := os.ReadFile(configPath)
	if err != nil {
		return "", err
	}

	layers := make([]string, 0)
	type layerData struct {
		name string
		data []byte
	}
	entries := make([]layerData, 0)

	for _, layer := range jsonArrayOf(manifest.Object, "layers") {
		if layer.kind != kindObject {
			return "", fmt.Errorf("%s: layer is not a JSON object", dir)
		}
		layerDigest, ok := fieldString(layer.obj, "digest")
		if !ok {
			return "", fmt.Errorf("%s: layer has no digest", dir)
		}
		mediaType, _ := fieldString(layer.obj, "mediaType")
		name := strings.TrimPrefix(layerDigest, "sha256:") + "/layer.tar"
		path, err := blobPath(dir, layerDigest)
		if err != nil {
			return "", err
		}
		data, err := readPossiblyGzip(path, strings.HasSuffix(mediaType, "+gzip"))
		if err != nil {
			return "", err
		}
		entries = append(entries, layerData{name: name, data: data})
		layers = append(layers, name)
	}

	manifestJSON := &jsonValue{kind: kindArray, arr: []*jsonValue{{
		kind: kindObject,
		obj: func() *orderedObject {
			entry := newJSONObject()
			entry.set("Config", &jsonValue{kind: kindString, str: configName})
			repos := make([]*jsonValue, 0, 1)
			repos = append(repos, &jsonValue{kind: kindString, str: ref})
			entry.set("RepoTags", &jsonValue{kind: kindArray, arr: repos})
			layersValue := make([]*jsonValue, 0, len(layers))
			for _, layer := range layers {
				layersValue = append(layersValue, &jsonValue{kind: kindString, str: layer})
			}
			entry.set("Layers", &jsonValue{kind: kindArray, arr: layersValue})
			return entry
		}(),
	}}}

	archive, err := newArchive(output)
	if err != nil {
		return "", err
	}
	closeWith := archive.CloseWith

	if err := archive.AddFile(configName, configBytes); err != nil {
		return "", closeWith(err)
	}
	for _, entry := range entries {
		if err := archive.AddFile(entry.name, entry.data); err != nil {
			return "", closeWith(err)
		}
	}
	if err := archive.AddFile("manifest.json", dumpJSONCompact(manifestJSON)); err != nil {
		return "", closeWith(err)
	}
	if err := closeWith(nil); err != nil {
		return "", err
	}
	return manifest.Digest, nil
}

// listBlobs returns every regular file under `<dir>/blobs` as a sorted
// slash-separated path relative to the layout.
func listBlobs(dir string) ([]string, error) {
	root := filepath.Join(dir, "blobs")
	blobs := make([]string, 0)
	err := filepath.Walk(root, func(path string, info os.FileInfo, err error) error {
		if err != nil {
			return err
		}
		if info.IsDir() {
			return nil
		}
		relative, err := filepath.Rel(dir, path)
		if err != nil {
			return err
		}
		blobs = append(blobs, filepath.ToSlash(relative))
		return nil
	})
	if err != nil {
		return nil, err
	}
	sort.Strings(blobs)
	return blobs, nil
}

// readPossiblyGzip reads data, decompressing a gzip stream when asked.
func readPossiblyGzip(path string, compressed bool) ([]byte, error) {
	raw, err := os.ReadFile(path)
	if err != nil {
		return nil, err
	}
	if !compressed {
		return raw, nil
	}
	reader, err := gzip.NewReader(bytes.NewReader(raw))
	if err != nil {
		return nil, err
	}
	decoded, err := io.ReadAll(reader)
	if err != nil {
		return nil, err
	}
	if err := reader.Close(); err != nil {
		return nil, err
	}
	return decoded, nil
}
