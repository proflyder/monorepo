#!/usr/bin/env python3
"""Temporarily materialize owner credentials from OpenBao into secrets/.

The local secrets/ directory keeps only a catalogue (secrets/catalog.json:
original relative file -> OpenBao KV path, field and encoding; no values).
Scripts that still read files such as secrets/home/ingress.env run after

    materialize.py home/ingress.env          # or --all, or a directory prefix
    ...                                      # use the files
    materialize.py --clean home/ingress.env  # or --clean --all

Auth: BAO_ADDR, else secrets/openbao/addr; BAO_TOKEN, else
secrets/openbao/operator.token (policy operator-kv reads secret/{ci,k8s,infra}/*).
Every run renews the periodic operator token, so regular use keeps it alive. Values never go to
stdout; files are written 0600 in 0700 directories.
"""
from __future__ import annotations

import argparse
import base64
import gzip
import io
import json
import os
import pathlib
import shutil
import sys
import tarfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[4]
SECRETS = ROOT / "secrets"

TEXT_ENCODINGS = ("text", "text+nl", "b64", "to_b64", "to_b64+nl", "gzip_b64")


def decode(value: str, encoding: str) -> bytes:
    """Rebuild the original file bytes from a KV string value."""
    raw = value.encode()
    if encoding == "text":
        return raw
    if encoding == "text+nl":
        return raw + b"\n"
    if encoding == "b64":
        return base64.b64decode(value)
    if encoding == "to_b64":
        return base64.b64encode(raw)
    if encoding == "to_b64+nl":
        return base64.b64encode(raw) + b"\n"
    if encoding == "gzip_b64":
        return gzip.decompress(base64.b64decode(value))
    raise ValueError(f"unknown encoding {encoding!r}")


def select(catalog: list[dict], names: list[str], everything: bool) -> list[dict]:
    if everything:
        return [e for e in catalog if e.get("file")]
    chosen = []
    for name in names:
        name = name.removeprefix("secrets/").rstrip("/")
        hits = [e for e in catalog if e.get("file") and
                (e["file"] == name or e["file"].startswith(name + "/"))]
        if not hits:
            raise SystemExit(f"not in catalogue: {name}")
        chosen.extend(hits)
    return chosen


def safe_target(base: pathlib.Path, rel: str) -> pathlib.Path:
    target = (base / rel).resolve()
    if base.resolve() not in target.parents:
        raise SystemExit(f"refusing path outside {base}: {rel}")
    return target


def write_private(path: pathlib.Path, data: bytes) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "wb") as handle:
        handle.write(data)
    os.chmod(path, 0o600)


def extract_tar(data: bytes, dest_parent: pathlib.Path) -> None:
    dest_parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        archive.extractall(dest_parent, filter="data")


class Bao:
    def __init__(self, addr: str, token: str):
        self.addr, self.token = addr.rstrip("/"), token
        self.cache: dict[str, dict] = {}

    def field(self, path: str, field: str) -> str:
        if path not in self.cache:
            req = urllib.request.Request(f"{self.addr}/v1/secret/data/{path}",
                                         headers={"X-Vault-Token": self.token})
            with urllib.request.urlopen(req, timeout=20) as resp:
                self.cache[path] = json.load(resp)["data"]["data"]
        return self.cache[path][field]


def address() -> str | None:
    if os.environ.get("BAO_ADDR"):
        return os.environ["BAO_ADDR"]
    path = SECRETS / "openbao" / "addr"
    return path.read_text().strip() if path.is_file() else None


def renew(addr: str, bao_token: str) -> None:
    """Best effort: extend the periodic token (default policy allows renew-self)."""
    req = urllib.request.Request(f"{addr.rstrip('/')}/v1/auth/token/renew-self",
                                 data=b"{}", method="POST",
                                 headers={"X-Vault-Token": bao_token})
    try:
        urllib.request.urlopen(req, timeout=20).close()
    except (urllib.error.URLError, OSError):
        print("warning: token renew-self failed", file=sys.stderr)


def token() -> str:
    if os.environ.get("BAO_TOKEN"):
        return os.environ["BAO_TOKEN"]
    return (SECRETS / "openbao" / "operator.token").read_text().strip()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("files", nargs="*", help="catalogue file or directory prefix")
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--clean", action="store_true", help="delete instead of write")
    ap.add_argument("--catalog", type=pathlib.Path, default=SECRETS / "catalog.json")
    ap.add_argument("--dest", type=pathlib.Path, default=SECRETS)
    args = ap.parse_args(argv)
    if not args.files and not args.all:
        ap.error("name files or pass --all")
    catalog = json.loads(args.catalog.read_text())["entries"]
    entries = select(catalog, args.files, args.all)
    if args.clean:
        for entry in entries:
            target = safe_target(args.dest, entry["file"])
            if entry["encoding"] == "tar_b64":
                shutil.rmtree(target, ignore_errors=True)
            else:
                target.unlink(missing_ok=True)
            print(f"removed {entry['file']}")
        return 0
    addr = address()
    if not addr:
        ap.error("set BAO_ADDR or write secrets/openbao/addr (https://vault.<home-domain>)")
    bao_token = token()
    renew(addr, bao_token)
    bao = Bao(addr, bao_token)
    for entry in entries:
        value = bao.field(entry["path"], entry["field"])
        target = safe_target(args.dest, entry["file"])
        if entry["encoding"] == "tar_b64":
            extract_tar(base64.b64decode(value), target.parent)
        else:
            write_private(target, decode(value, entry["encoding"]))
        print(f"wrote {entry['file']} <- {entry['path']}#{entry['field']}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
