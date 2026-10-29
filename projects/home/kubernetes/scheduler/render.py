#!/usr/bin/env python3
"""Render Kubernetes/nft templates with private values.

Only placeholders declared in ALLOWED are substituted. Unknown placeholders or
missing values are hard errors, so a half-rendered manifest can never be
applied. Rendered output must go to an ignored/private directory.

Usage:
  render.py --env /path/to/scheduler.env --out /path/to/rendered \
            manifest.yaml [more files...]
"""
from __future__ import annotations

import argparse
import pathlib
import re
import sys

ALLOWED = {
    "BUILD_ID",
    "HA_URL",
    "LAN_CIDR",
    "NODE_LAN_IP",
    "SCHEDULER_NODE_PORT",
    "SCHEDULER_PUBLIC_URL",
    "TAILNET_SCHED_HOSTNAME",
    "TAILNET_CONTROL_HOSTNAME",
    "CONTROL_NODE_PORT",
}

PLACEHOLDER = re.compile(r"\$\{([A-Z0-9_]+)\}")


def parse_env(path: pathlib.Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for lineno, raw in enumerate(path.read_text().splitlines(), 1):
        line = raw.strip()
        if not line or line.startswith("#"):
            continue
        if "=" not in line:
            raise SystemExit(f"{path}:{lineno}: expected KEY=VALUE")
        key, value = line.split("=", 1)
        key = key.strip()
        value = value.strip().strip('"').strip("'")
        if key in values:
            raise SystemExit(f"{path}:{lineno}: duplicate key {key}")
        values[key] = value
    return values


def render(text: str, values: dict[str, str], origin: str) -> str:
    def replace(match: re.Match[str]) -> str:
        key = match.group(1)
        if key not in ALLOWED:
            raise SystemExit(f"{origin}: placeholder ${{{key}}} is not allowed")
        if not values.get(key):
            raise SystemExit(f"{origin}: missing value for ${{{key}}}")
        return values[key]

    rendered = PLACEHOLDER.sub(replace, text)
    leftover = PLACEHOLDER.search(rendered)
    if leftover:
        raise SystemExit(f"{origin}: unrendered placeholder {leftover.group(0)}")
    return rendered


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--env", required=True, type=pathlib.Path)
    parser.add_argument("--out", required=True, type=pathlib.Path)
    parser.add_argument("--set", action="append", default=[], metavar="KEY=VALUE")
    parser.add_argument("files", nargs="+", type=pathlib.Path)
    args = parser.parse_args()

    values = parse_env(args.env)
    for item in args.set:
        key, _, value = item.partition("=")
        if key not in ALLOWED:
            raise SystemExit(f"--set {key}: not an allowed placeholder")
        values[key] = value

    args.out.mkdir(parents=True, exist_ok=True)
    for src in args.files:
        text = src.read_text()
        rendered = render(text, values, str(src))
        dest = args.out / src.name
        dest.write_text(rendered)
        print(f"rendered {src} -> {dest}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
