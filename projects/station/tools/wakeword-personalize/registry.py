#!/usr/bin/env python3
"""Append-only local model snapshots. Registry data must remain outside public Git."""
import argparse
from datetime import datetime, timezone
import fcntl
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import uuid


def digest(path):
    h = hashlib.sha256()
    with path.open("rb") as stream:
        while block := stream.read(1024 * 1024):
            h.update(block)
    return h.hexdigest()


def snapshot(registry, version, source, metadata=None):
    registry, source = Path(registry).resolve(), Path(source).resolve()
    if not re.fullmatch(r"[a-z0-9][a-z0-9._-]{0,95}", version) or version in {".", ".."}:
        raise ValueError("Invalid version ID")
    if source == registry or source in registry.parents:
        raise ValueError("Source must not contain the registry")
    registry.mkdir(parents=True, exist_ok=True, mode=0o700)
    registry.chmod(0o700)
    with (registry / ".registry.lock").open("a") as lock:
        os.chmod(lock.name, 0o600)
        fcntl.flock(lock, fcntl.LOCK_EX)
        destination = registry / version
        if destination.exists():
            raise FileExistsError("Version already exists; snapshots are never overwritten")
        stage = registry / (".stage-" + uuid.uuid4().hex)
        stage.mkdir(mode=0o700)
        try:
            files = {}
            for src in sorted(source.rglob("*")):
                if src.is_symlink():
                    raise ValueError("Symlinks are not accepted in model snapshots")
                if not src.is_file():
                    continue
                rel = src.relative_to(source)
                if str(rel) in {"VERSION.json", "SHA256SUMS"}:
                    raise ValueError("Reserved snapshot filename")
                dst = stage / rel
                dst.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
                shutil.copyfile(src, dst)
                dst.chmod(0o600)
                sha = digest(dst)
                if sha != digest(src):
                    raise RuntimeError("Source changed while copying")
                files[str(rel)] = {"sha256": sha, "bytes": dst.stat().st_size}
            if not files:
                raise ValueError("Empty model snapshot")
            entry = {"version": version, "created_utc": datetime.now(timezone.utc).isoformat(),
                     "metadata": metadata or {}, "files": files}
            manifest = stage / "VERSION.json"
            manifest.write_text(json.dumps(entry, ensure_ascii=False, indent=2))
            manifest.chmod(0o600)
            sums = {name: obj["sha256"] for name, obj in files.items()}
            sums["VERSION.json"] = digest(manifest)
            (stage / "SHA256SUMS").write_text("".join(sha + "  " + name + "\n" for name, sha in sorted(sums.items())))
            (stage / "SHA256SUMS").chmod(0o600)
            stage.rename(destination)
            index = {"schema": 1, "versions": []}
            for directory in sorted(registry.iterdir()):
                if directory.is_dir() and not directory.name.startswith(".") and (directory / "VERSION.json").is_file():
                    item = json.loads((directory / "VERSION.json").read_text())
                    index["versions"].append({"version": item["version"], "created_utc": item["created_utc"],
                                              "metadata": item["metadata"], "files": len(item["files"])})
            temp = registry / ".index.tmp"
            temp.write_text(json.dumps(index, ensure_ascii=False, indent=2))
            temp.chmod(0o600)
            temp.replace(registry / "index.json")
            return destination
        finally:
            if stage.exists():
                shutil.rmtree(stage)


def verify(path):
    path = Path(path).resolve()
    entry = json.loads((path / "VERSION.json").read_text())
    for name, expected in entry["files"].items():
        original = path / name
        file = original.resolve()
        if not file.is_relative_to(path) or original.is_symlink() or digest(file) != expected["sha256"] or file.stat().st_size != expected["bytes"]:
            raise ValueError("Snapshot checksum mismatch")
    for line in (path / "SHA256SUMS").read_text().splitlines():
        sha, name = line.split("  ", 1)
        file = (path / name).resolve()
        if not file.is_relative_to(path) or digest(file) != sha:
            raise ValueError("Checksum manifest mismatch")
    return len(entry["files"])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--registry", required=True, type=Path)
    parser.add_argument("--version", required=True)
    parser.add_argument("--from-dir", type=Path)
    parser.add_argument("--metadata", type=Path)
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    if args.verify:
        print("Verified files:", verify(args.registry / args.version))
    else:
        if not args.from_dir:
            parser.error("--from-dir is required when registering")
        metadata = json.loads(args.metadata.read_text()) if args.metadata else {}
        path = snapshot(args.registry, args.version, args.from_dir, metadata)
        print("Saved version:", path.name, "files:", verify(path))


if __name__ == "__main__":
    main()
