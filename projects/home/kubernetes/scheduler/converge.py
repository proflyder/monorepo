#!/usr/bin/env python3
"""Converge a Deployment to its rendered manifest.

`kubectl apply`/`kubectl patch` only add or update the fields they carry: a
volume or volumeMount that an earlier ad-hoc patch added stays in the live
object forever (the 2026-10 hostPath `/opt/ricardo/current/bin` drift). This
helper renders the desired object and replaces the live Deployment with it, so
fields absent from the manifest are actually removed.

Runtime state that the manifest deliberately does not own is carried over from
the live object before the replace:

  --preserve-replicas   keep the current replica count (mode/scaling is manual)
  --preserve-env        keep the current container env/envFrom (execute flags)

Everything else (pod template, volumes, probes, resources, image) comes from
the rendered manifest. The current resourceVersion is reused so a concurrent
change fails the replace instead of being overwritten.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys
import tempfile

import yaml


def _run(kubectl: str, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run([kubectl, *args], text=True, capture_output=True, check=False)


def desired_objects(path: pathlib.Path) -> list[dict]:
    return [doc for doc in yaml.safe_load_all(path.read_text()) if doc]


def desired_deployment(objects: list[dict], path: pathlib.Path) -> dict:
    deployments = [doc for doc in objects if doc.get("kind") == "Deployment"]
    if len(deployments) != 1:
        raise SystemExit(f"{path}: expected exactly one Deployment, found {len(deployments)}")
    return deployments[0]


def apply_support_objects(kubectl: str, namespace: str, objects: list[dict]) -> None:
    """Apply the Service/PVC in the same file; only the Deployment is converged."""
    if not objects:
        return
    with tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False) as handle:
        yaml.safe_dump_all(objects, handle)
        path = pathlib.Path(handle.name)
    try:
        result = _run(kubectl, "-n", namespace, "apply", "-f", str(path))
    finally:
        path.unlink(missing_ok=True)
    if result.returncode != 0:
        raise SystemExit(f"support objects: apply failed: {result.stderr.strip()}")


def live_deployment(kubectl: str, namespace: str, name: str) -> dict | None:
    result = _run(kubectl, "-n", namespace, "get", "deployment", name, "-o", "json")
    if result.returncode != 0:
        return None
    return json.loads(result.stdout)


def merge_runtime_state(
    desired: dict,
    current: dict,
    *,
    preserve_replicas: bool,
    preserve_env: bool,
) -> None:
    if preserve_replicas and "replicas" in current.get("spec", {}):
        desired["spec"]["replicas"] = current["spec"]["replicas"]

    if preserve_env:
        current_containers = {
            container.get("name"): container
            for container in current.get("spec", {}).get("template", {}).get("spec", {}).get("containers", [])
        }
        for container in desired["spec"]["template"]["spec"].get("containers", []):
            live = current_containers.get(container.get("name"))
            if not live:
                continue
            for key in ("env", "envFrom"):
                # An empty live list means the container has no runtime state to
                # keep; leave the manifest value in place then.
                if live.get(key):
                    container[key] = live[key]

    resource_version = current.get("metadata", {}).get("resourceVersion")
    if resource_version:
        desired.setdefault("metadata", {})["resourceVersion"] = resource_version


def converge(
    kubectl: str,
    namespace: str,
    name: str,
    manifest: pathlib.Path,
    *,
    preserve_replicas: bool = False,
    preserve_env: bool = False,
) -> str:
    objects = desired_objects(manifest)
    desired = desired_deployment(objects, manifest)
    apply_support_objects(
        kubectl,
        namespace,
        [doc for doc in objects if doc.get("kind") != "Deployment"],
    )
    current = live_deployment(kubectl, namespace, name)

    if current is None:
        with tempfile.NamedTemporaryFile("w", suffix=".yaml", delete=False) as handle:
            yaml.safe_dump(desired, handle)
            path = pathlib.Path(handle.name)
        try:
            result = _run(kubectl, "-n", namespace, "create", "-f", str(path))
        finally:
            path.unlink(missing_ok=True)
        if result.returncode != 0:
            raise SystemExit(f"{name}: create failed: {result.stderr.strip()}")
        return f"{name}: created from manifest"

    merge_runtime_state(
        desired,
        current,
        preserve_replicas=preserve_replicas,
        preserve_env=preserve_env,
    )

    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as handle:
        json.dump(desired, handle)
        path = pathlib.Path(handle.name)
    try:
        result = _run(kubectl, "-n", namespace, "replace", "-f", str(path))
    finally:
        path.unlink(missing_ok=True)
    if result.returncode != 0:
        raise SystemExit(f"{name}: replace failed: {result.stderr.strip()}")
    return f"{name}: converged to manifest"


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--kubectl", default="kubectl")
    parser.add_argument("--namespace", default="home-assistant")
    parser.add_argument("--name", required=True)
    parser.add_argument("--file", required=True, type=pathlib.Path)
    parser.add_argument("--preserve-replicas", action="store_true")
    parser.add_argument("--preserve-env", action="store_true")
    args = parser.parse_args(argv)
    print(
        converge(
            args.kubectl,
            args.namespace,
            args.name,
            args.file,
            preserve_replicas=args.preserve_replicas,
            preserve_env=args.preserve_env,
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
