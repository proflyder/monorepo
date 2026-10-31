#!/usr/bin/env python3
"""Restore Deployments that this deploy actually changed.

`deploy.sh` records, immediately before it applies a change:
  <name>.before.json   the full Deployment object as it was
  <name>.absent        the Deployment did not exist
and writes `<name>.applied` only after the apply/patch succeeded.

This script restores from that record instead of running
`kubectl rollout undo`:

* a failure before `apply` (image build/push, rendering, preflight) leaves no
  `.applied` marker, so nothing is rolled back;
* a failure after `apply` restores the exact remembered object (image, env,
  replicas, probes, volumes) with a full replace — not "one revision back",
  which could undo an unrelated change made by an earlier deploy step, and not
  a merge patch, which cannot remove fields added by the failed deploy;
* if the Deployment generation did not change, no request is sent at all.

Usage:
  rollback.py --state-dir DIR [--namespace home-assistant] [--kubectl kubectl]
"""
from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys
import tempfile


def _run(kubectl: str, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run([kubectl, *args], text=True, capture_output=True, check=False)


def generation(kubectl: str, namespace: str, name: str) -> int | None:
    result = _run(
        kubectl, "-n", namespace, "get", "deployment", name, "-o",
        "jsonpath={.metadata.generation}",
    )
    if result.returncode != 0 or not result.stdout.strip():
        return None
    try:
        return int(result.stdout.strip())
    except ValueError:
        return None


def restore_object(before: dict) -> dict:
    """Strip read-only fields so the recorded object can be PUT back as-is."""
    obj = json.loads(json.dumps(before))
    obj.pop("status", None)
    metadata = obj.get("metadata", {})
    for key in ("managedFields", "generation", "creationTimestamp", "selfLink", "uid"):
        metadata.pop(key, None)
    # An empty resourceVersion makes the update unconditional; the recorded
    # revision is gone by definition after a failed deploy.
    metadata.pop("resourceVersion", None)
    return obj


def restore_one(kubectl: str, namespace: str, state: pathlib.Path, name: str) -> str:
    marker_applied = state / f"{name}.applied"
    before_path = state / f"{name}.before.json"
    absent_marker = state / f"{name}.absent"

    if not marker_applied.exists():
        return f"{name}: not applied by this job, skip"

    if absent_marker.exists():
        result = _run(kubectl, "-n", namespace, "delete", "deployment", name, "--ignore-not-found")
        if result.returncode != 0:
            return f"{name}: delete failed: {result.stderr.strip()}"
        return f"{name}: was absent before, deleted"

    if not before_path.exists():
        return f"{name}: applied but no recorded state, skip"

    before = json.loads(before_path.read_text())
    try:
        before_generation = int(before["metadata"]["generation"])
    except (KeyError, TypeError, ValueError):
        return f"{name}: recorded state has no generation, skip"

    current = generation(kubectl, namespace, name)
    if current is None:
        return f"{name}: deployment not found, skip"
    if current <= before_generation:
        return f"{name}: generation {current} did not change (was {before_generation}), skip"

    patch = restore_object(before)
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
        json.dump(patch, handle)
        patch_path = handle.name
    try:
        result = _run(
            kubectl, "-n", namespace, "replace", "-f", patch_path,
        )
    finally:
        pathlib.Path(patch_path).unlink(missing_ok=True)
    if result.returncode != 0:
        return f"{name}: restore replace failed: {result.stderr.strip()}"

    rollout = _run(kubectl, "-n", namespace, "rollout", "status", f"deployment/{name}", "--timeout=180s")
    if rollout.returncode != 0:
        return f"{name}: restored to generation {before_generation}, rollout not ready: {rollout.stderr.strip()}"
    return f"{name}: restored to generation {before_generation}"


def rollback(kubectl: str, namespace: str, state: pathlib.Path) -> list[str]:
    if not state.is_dir():
        return ["no deploy state recorded, nothing to roll back"]
    markers = sorted(state.glob("*.applied"))
    if not markers:
        return ["no deployment was applied by this job, nothing to roll back"]
    messages = []
    for marker in markers:
        name = marker.name[: -len(".applied")]
        messages.append(restore_one(kubectl, namespace, state, name))
    return messages


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--state-dir", required=True, type=pathlib.Path)
    parser.add_argument("--namespace", default="home-assistant")
    parser.add_argument("--kubectl", default="kubectl")
    args = parser.parse_args(argv)
    for message in rollback(args.kubectl, args.namespace, args.state_dir):
        print(message)
    return 0


if __name__ == "__main__":
    sys.exit(main())
