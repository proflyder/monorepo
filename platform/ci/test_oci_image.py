#!/usr/bin/env python3
"""Unit tests for the OCI layout helper used by CI and deploy.sh."""
from __future__ import annotations

import gzip
import io
import json
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import oci_image  # noqa: E402


def make_layout(root: Path, layer_bytes: bytes) -> tuple[str, str]:
    """Write a minimal single-layer OCI layout; return (manifest, config) digests."""
    import hashlib

    def blob(data: bytes) -> str:
        digest = hashlib.sha256(data).hexdigest()
        path = root / "blobs" / "sha256" / digest
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
        return "sha256:" + digest

    layer = blob(gzip.compress(layer_bytes))
    config = blob(json.dumps({"architecture": "amd64", "os": "linux"}).encode())
    manifest = {
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.manifest.v1+json",
        "config": {
            "mediaType": "application/vnd.oci.image.config.v1+json",
            "size": 48,
            "digest": config,
        },
        "layers": [
            {
                "mediaType": "application/vnd.oci.image.layer.v1.tar+gzip",
                "size": 10,
                "digest": layer,
            }
        ],
    }
    manifest_bytes = json.dumps(manifest).encode()
    manifest_digest = blob(manifest_bytes)
    index = {
        "schemaVersion": 2,
        "mediaType": "application/vnd.oci.image.index.v1+json",
        "manifests": [
            {
                "mediaType": "application/vnd.oci.image.manifest.v1+json",
                "size": len(manifest_bytes),
                "digest": manifest_digest,
            }
        ],
    }
    (root / "index.json").write_text(json.dumps(index))
    (root / "oci-layout").write_text('{"imageLayoutVersion": "1.0.0"}')
    return manifest_digest, config


class OciImageTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name) / "layout"
        self.root.mkdir()
        inner = io.BytesIO()
        with tarfile.open(fileobj=inner, mode="w") as archive:
            info = tarfile.TarInfo("hello.txt")
            data = b"hello"
            info.size = len(data)
            archive.addfile(info, io.BytesIO(data))
        self.layer = inner.getvalue()
        self.manifest, self.config = make_layout(self.root, self.layer)

    def tearDown(self):
        self.tmp.cleanup()

    def test_digest_is_the_manifest_digest(self):
        self.assertEqual(oci_image.manifest_digest(self.root)[0], self.manifest)

    def test_pack_writes_an_oci_archive_with_ref_annotation(self):
        out = Path(self.tmp.name) / "image.tar"
        digest = oci_image.pack(self.root, "ghcr.io/example/app:tag", out)
        self.assertEqual(digest, self.manifest)
        with tarfile.open(out) as archive:
            names = archive.getnames()
            index = json.load(archive.extractfile("index.json"))
        self.assertIn("oci-layout", names)
        self.assertIn("blobs/sha256/" + self.config.split(":", 1)[1], names)
        annotation = index["manifests"][0]["annotations"]["org.opencontainers.image.ref.name"]
        self.assertEqual(annotation, "ghcr.io/example/app:tag")

    def test_docker_archive_uncompresses_layers(self):
        out = Path(self.tmp.name) / "image-docker.tar"
        oci_image.docker_archive(self.root, "ghcr.io/example/app:tag", out)
        with tarfile.open(out) as archive:
            manifest = json.load(archive.extractfile("manifest.json"))[0]
            layer = archive.extractfile(manifest["Layers"][0]).read()
        self.assertEqual(layer, self.layer)
        self.assertEqual(manifest["RepoTags"], ["ghcr.io/example/app:tag"])


if __name__ == "__main__":
    unittest.main()
