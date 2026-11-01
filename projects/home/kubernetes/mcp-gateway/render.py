#!/usr/bin/env python3
"""Render the gateway manifests with the shared scheduler renderer.

`projects/home/kubernetes/scheduler/render.py` owns the placeholder policy:
every `${KEY}` must be declared in its `ALLOWED` set and must have a value, so
a half-rendered manifest can never be applied. The gateway adds the
placeholders the shared renderer does not know (`MCP_GATEWAY_IMAGE`, `HAOS_IP`
and `STATION_IP`, plus the shared `BUILD_ID`, `LAN_CIDR` and `NODE_LAN_IP`), so
this module imports that renderer, extends `ALLOWED` with the gateway-only keys
and delegates. Nothing else is duplicated: when the scheduler set grows, the
union here follows automatically.

`manifests/catalog.yaml` is rendered too: external checks contain host
placeholders, so the ConfigMap is applied from the rendered copy, never from
the tracked file.

Usage is identical to the scheduler renderer:

  render.py --env PRIVATE_ENV --out RENDER_DIR [--set KEY=VALUE] manifest...
"""
from __future__ import annotations

import pathlib
import sys

SCHEDULER_DIR = pathlib.Path(__file__).resolve().parents[1] / "scheduler"
sys.path.insert(0, str(SCHEDULER_DIR))

import render as scheduler_render  # noqa: E402  (path set above on purpose)

GATEWAY_PLACEHOLDERS = {"MCP_GATEWAY_IMAGE", "HAOS_IP", "STATION_IP"}
scheduler_render.ALLOWED |= GATEWAY_PLACEHOLDERS


def main() -> int:
    return scheduler_render.main()


if __name__ == "__main__":
    sys.exit(main())
