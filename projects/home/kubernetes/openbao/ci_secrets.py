#!/usr/bin/env python3
"""Materialise CI secrets fetched from OpenBao into private files.

`deploy-home.yml` logs in to OpenBao over GitHub OIDC, reads
`secret/ci/deploy-home` and hands the JSON response to this script. The script
writes the requested values to `$RUNNER_TEMP/ci` with mode 600 and emits
`::add-mask::` workflow commands for their non-trivial lines so later steps
cannot leak them into the log. It never prints a value as a normal log line.

Values are stored in OpenBao as the original text (not base64), so the files
written here are byte-identical to the files the GitHub secrets used to carry:

    python3 ci_secrets.py --payload response.json --dir "$CI_DIR" \
        --key kubeconfig=kubeconfig --key scheduler_env=scheduler.env

The script is deliberately transport-agnostic: the payload may be either a
full KV v2 response (``{"data": {"data": {...}}}``) or the inner mapping.
"""
from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
from pathlib import Path

# Short lines (for example ``true`` or ``home``) would mask unrelated log text.
MASK_MIN_LENGTH = 8


def load_payload(path: Path) -> dict[str, str]:
    """Return the KV v2 data mapping from a JSON response file."""
    raw = json.loads(path.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        raise ValueError("OpenBao response is not a JSON object")
    if raw.get("errors"):
        raise ValueError("OpenBao response reports an error, not secret data")
    inner = raw.get("data")
    if isinstance(inner, dict):
        data = inner.get("data") if isinstance(inner.get("data"), dict) else inner
    else:
        data = raw
    if not isinstance(data, dict) or not data:
        raise ValueError("OpenBao response has no data mapping")
    return data


def mask_values(value: str) -> list[str]:
    """Lines that must be registered with ``::add-mask::`` for this value."""
    lines = value.splitlines() if "\n" in value else [value]
    return [line for line in lines if len(line.strip()) >= MASK_MIN_LENGTH]


def _safe_name(name: str) -> str:
    candidate = Path(name)
    if candidate.is_absolute() or ".." in candidate.parts or name.strip() != name:
        raise ValueError(f"unsafe target file name: {name!r}")
    if not name or name != candidate.name:
        raise ValueError(f"target must be a plain file name: {name!r}")
    return name


def materialise(data: dict[str, str], target: Path,
                mapping: list[tuple[str, str]]) -> list[str]:
    """Write each ``data_key`` value to ``target/filename`` with mode 600."""
    missing = [key for key, _ in mapping if not isinstance(data.get(key), str)
               or not data[key]]
    if missing:
        raise ValueError("OpenBao secret is missing required key(s): "
                         + ", ".join(sorted(missing)))

    target.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(target, 0o700)
    written: list[str] = []
    for key, filename in mapping:
        _safe_name(filename)
        content = data[key].encode("utf-8")
        handle, temporary = tempfile.mkstemp(dir=target, prefix=".secret-")
        try:
            os.fchmod(handle, 0o600)
            with os.fdopen(handle, "wb") as stream:
                stream.write(content)
            os.replace(temporary, target / filename)
        except BaseException:
            Path(temporary).unlink(missing_ok=True)
            raise
        os.chmod(target / filename, 0o600)
        written.append(filename)
    return written


def parse_key(value: str) -> tuple[str, str]:
    key, separator, filename = value.partition("=")
    if not separator or not key or not filename:
        raise argparse.ArgumentTypeError("expected data_key=filename")
    return key, filename


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--payload", required=True, type=Path,
                        help="JSON response from secret/data/ci/deploy-home")
    parser.add_argument("--dir", required=True, type=Path,
                        help="private directory for the materialised files")
    parser.add_argument("--key", required=True, action="append", type=parse_key,
                        help="data key and target file name (data_key=filename)")
    args = parser.parse_args(argv)

    try:
        data = load_payload(args.payload)
        written = materialise(data, args.dir, args.key)
    except (OSError, ValueError) as error:
        sys.exit(f"ci_secrets: {error}")

    masks: list[str] = []
    for key, _ in args.key:
        for line in mask_values(data[key]):
            if line not in masks:
                masks.append(line)
    for line in masks:
        print(f"::add-mask::{line}")
    print(f"ci_secrets: materialised {len(written)} file(s) "
          f"({', '.join(written)}); registered {len(masks)} mask(s)")
    return 0


if __name__ == "__main__":
    main()
