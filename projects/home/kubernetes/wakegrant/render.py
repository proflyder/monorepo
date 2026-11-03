#!/usr/bin/env python3
"""Render the wakegrant manifests with the shared scheduler renderer.

`projects/home/kubernetes/scheduler/render.py` owns the placeholder policy:
every `${KEY}` must be declared in its `ALLOWED` set and must have a value, so
a half-rendered manifest can never be applied. This module adds the
wakegrant-only keys and delegates, so the shared set follows automatically.

Usage:

  render.py --env PRIVATE_ENV --out RENDER_DIR [--set KEY=VALUE] manifest...
"""
from __future__ import annotations

import pathlib
import sys

SCHEDULER_DIR = pathlib.Path(__file__).resolve().parents[1] / "scheduler"
sys.path.insert(0, str(SCHEDULER_DIR))

import render as scheduler_render  # noqa: E402  (path set above on purpose)

WAKEGRANT_PLACEHOLDERS = {
    "WAKE_GRANT_IMAGE",
    "WAKE_GRANT_NODE_PORT",
}
scheduler_render.ALLOWED |= WAKEGRANT_PLACEHOLDERS


def main() -> int:
    return scheduler_render.main()


if __name__ == "__main__":
    sys.exit(main())
