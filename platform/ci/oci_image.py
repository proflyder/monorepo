#!/usr/bin/env python3
"""Inspect and package the OCI image layouts produced by `rules_oci`.

`rules_oci` writes a plain OCI image layout directory (`index.json` + `blobs/`).
The `oci_load` helper is not usable when the image platform (linux/amd64)
differs from the build host (e.g. macOS): it selects the linux `tar` toolchain.
This script therefore does the two platform-independent operations we need:

  digest  print the manifest digest of the (single) image in a layout
  pack    write a standard OCI archive with a repo tag annotation, suitable for
          `ctr images import` on the K3s node
  docker  write a docker-archive (`docker load`) from the same layout, for
          local smoke tests on a workstation

All three are byte-preserving: they copy blobs, they never rewrite them, so the
digest printed by `digest` is exactly the digest that a registry stores after
the archive is pushed.
"""
from __future__ import annotations

import argparse
import gzip
import io
import json
import pathlib
import sys
import tarfile

INDEX = "index.json"


def _read_json(path: pathlib.Path) -> dict:
    with path.open("rb") as handle:
        return json.load(handle)


def _blob(layout: pathlib.Path, digest: str) -> pathlib.Path:
    algorithm, _, value = digest.partition(":")
    if algorithm != "sha256" or not value:
        raise SystemExit(f"unsupported digest: {digest}")
    return layout / "blobs" / algorithm / value


def manifest_digest(layout: pathlib.Path) -> tuple[str, dict]:
    index = _read_json(layout / INDEX)
    manifests = index.get("manifests") or []
    if len(manifests) != 1:
        raise SystemExit(f"{layout}: expected one image manifest, found {len(manifests)}")
    return manifests[0]["digest"], _read_json(_blob(layout, manifests[0]["digest"]))


def _add_bytes(archive: tarfile.TarFile, name: str, data: bytes) -> None:
    info = tarfile.TarInfo(name)
    info.size = len(data)
    info.mode = 0o644
    info.mtime = 0
    archive.addfile(info, io.BytesIO(data))


def pack(layout: pathlib.Path, ref: str, output: pathlib.Path) -> str:
    """OCI archive (tar of the layout) plus a ref annotation for ctr."""
    digest, _ = manifest_digest(layout)
    index = _read_json(layout / INDEX)
    index["manifests"][0].setdefault("annotations", {})[
        "org.opencontainers.image.ref.name"
    ] = ref
    with tarfile.open(output, "w", format=tarfile.PAX_FORMAT) as archive:
        _add_bytes(archive, "oci-layout", (layout / "oci-layout").read_bytes())
        _add_bytes(archive, INDEX, json.dumps(index, indent=2).encode())
        for path in sorted((layout / "blobs").rglob("*")):
            if path.is_file():
                relative = path.relative_to(layout).as_posix()
                _add_bytes(archive, relative, path.read_bytes())
    return digest


def docker_archive(layout: pathlib.Path, ref: str, output: pathlib.Path) -> str:
    """Docker `load` archive: uncompressed layers and a manifest.json."""
    digest, manifest = manifest_digest(layout)
    config_digest = manifest["config"]["digest"]
    config_name = config_digest.split(":", 1)[1] + ".json"
    config_bytes = _blob(layout, config_digest).read_bytes()

    entries = [{"Config": config_name, "RepoTags": [ref], "Layers": []}]
    with tarfile.open(output, "w", format=tarfile.PAX_FORMAT) as archive:
        _add_bytes(archive, config_name, config_bytes)
        for layer in manifest["layers"]:
            layer_digest = layer["digest"]
            name = layer_digest.split(":", 1)[1] + "/layer.tar"
            data = _blob(layout, layer_digest).read_bytes()
            if layer.get("mediaType", "").endswith("+gzip"):
                data = gzip.decompress(data)
            _add_bytes(archive, name, data)
            entries[0]["Layers"].append(name)
        _add_bytes(archive, "manifest.json", json.dumps(entries).encode())
    return digest


def main() -> int:
    parser = argparse.ArgumentParser()
    sub = parser.add_subparsers(dest="command", required=True)

    digest = sub.add_parser("digest")
    digest.add_argument("layout", type=pathlib.Path)

    for name in ("pack", "docker"):
        command = sub.add_parser(name)
        command.add_argument("layout", type=pathlib.Path)
        command.add_argument("--ref", required=True)
        command.add_argument("--out", required=True, type=pathlib.Path)

    args = parser.parse_args()
    if args.command == "digest":
        print(manifest_digest(args.layout)[0])
        return 0
    builder = pack if args.command == "pack" else docker_archive
    print(builder(args.layout, args.ref, args.out))
    return 0


if __name__ == "__main__":
    sys.exit(main())
