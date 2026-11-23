#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import secrets
import subprocess
import sys

import workstation as ws

PATH = "k8s/home-assistant/scheduler-auth"
NAME = "owner"
MODULES = [
    "base",
    "home",
    "scenarios",
    "alarms_timers",
    "intercom_call",
    "notifications",
    "chat",
    "vpn",
    "system",
    "infra_reports",
    "alerts",
    "admin",
]
SERVICE_MODULES: list[str] = []
KUBECTL_NAMESPACE = "home-assistant"


def digest(token: str) -> str:
    return hashlib.sha256(token.encode()).hexdigest()


def new_token() -> str:
    return f"home_{secrets.token_urlsafe(32)}"


def read_secret(bao: ws.Bao, path: str) -> tuple[dict, int]:
    body = bao.call("GET", f"secret/data/{path}")
    data = (body or {}).get("data") or {}
    return dict(data.get("data") or {}), int(data.get("metadata", {}).get("version", 0))


def decode_seed(value: str) -> dict:
    if not value.strip():
        return {"revision": 0, "tokens": []}
    seed = json.loads(value)
    if not isinstance(seed, dict) or not isinstance(seed.get("tokens"), list):
        raise ValueError("app_tokens must be an access snapshot")
    return seed


def merge_token(seed: dict, name: str, token: str) -> dict:
    entry = {
        "id": name,
        "name": name,
        "sha256": digest(token),
        "owner": True,
        "modules": MODULES,
    }
    tokens = [item for item in seed.get("tokens", []) if item.get("id") != name]
    tokens.append(entry)
    return {"revision": 0, "tokens": sorted(tokens, key=lambda item: item.get("id", ""))}



def legacy_consumers(legacy: str, secrets_doc: dict, configmaps_doc: dict) -> list[str]:
    if not legacy:
        return []
    found = []
    for item in secrets_doc.get("items", []):
        name = item.get("metadata", {}).get("name", "")
        if name == "scheduler-auth":
            continue
        for key, value in (item.get("data") or {}).items():
            try:
                raw = base64.b64decode(value)
            except (ValueError, TypeError):
                continue
            if legacy.encode() in raw:
                found.append(f"Secret/{name}:{key}")
    for item in configmaps_doc.get("items", []):
        name = item.get("metadata", {}).get("name", "")
        for key, value in (item.get("data") or {}).items():
            if isinstance(value, str) and legacy in value:
                found.append(f"ConfigMap/{name}:{key}")
    return sorted(found)


def preflight(bao: ws.Bao, path: str, run=None) -> list[str]:
    if run is None:
        run = subprocess.run
    values, _ = read_secret(bao, path)
    legacy = values.get("token", "")
    if not legacy:
        return []
    secrets_doc = json.loads(
        run(
            ["mise", "run", "kubectl", "--", "-n", KUBECTL_NAMESPACE, "get", "secrets", "-o", "json"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    configmaps_doc = json.loads(
        run(
            ["mise", "run", "kubectl", "--", "-n", KUBECTL_NAMESPACE, "get", "configmaps", "-o", "json"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    return legacy_consumers(legacy, secrets_doc, configmaps_doc)

def issue(bao: ws.Bao, path: str, name: str, token: str, dry_run: bool = False) -> dict:
    values, version = read_secret(bao, path)
    seed = decode_seed(values.get("app_tokens", ""))
    seed = merge_token(seed, name, token)
    values["app_tokens"] = json.dumps(seed, ensure_ascii=False, separators=(",", ":"))
    previous = values.get("service_token") or values.get("token")
    values["service_token"] = new_token()
    if previous:
        values["service_token_previous"] = previous
    else:
        values.pop("service_token_previous", None)
    if not dry_run:
        bao.call(
            "POST",
            f"secret/data/{path}",
            {"data": values, "options": {"cas": version}},
        )
    return values


def finalize(bao: ws.Bao, path: str, dry_run: bool = False) -> dict:
    values, version = read_secret(bao, path)
    values.pop("service_token_previous", None)
    if not dry_run:
        bao.call(
            "POST",
            f"secret/data/{path}",
            {"data": values, "options": {"cas": version}},
        )
    return values


def main(argv: list[str] | None = None, out=print) -> int:
    parser = argparse.ArgumentParser(
        description="Bootstrap the owner application token and the service token in OpenBao"
    )
    parser.add_argument("--name", default=NAME)
    parser.add_argument("--path", default=PATH)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--finalize", action="store_true")
    parser.add_argument("--skip-preflight", action="store_true")
    args = parser.parse_args(argv)
    if not args.name.strip() or "/" in args.name:
        raise SystemExit("name must be a non-empty plain string")
    bao = ws.operator(ws.SECRETS)
    if args.finalize:
        try:
            finalize(bao, args.path, args.dry_run)
        except ws.Error as error:
            raise SystemExit(str(error)) from None
        out("previous service token removed from OpenBao")
        return 0
    if not args.skip_preflight:
        try:
            consumers = preflight(bao, args.path)
        except (subprocess.CalledProcessError, ValueError) as error:
            raise SystemExit(f"preflight failed; no rotation was made: {error}") from None
        if consumers:
            raise SystemExit(
                "legacy service token is still referenced by: "
                + ", ".join(consumers)
                + "; update these consumers to scheduler-auth#service_token "
                "(or run with --skip-preflight at your own risk)"
            )
    token = new_token()
    try:
        issue(bao, args.path, args.name.strip(), token, args.dry_run)
    except ws.Error as error:
        raise SystemExit(str(error)) from None
    if args.dry_run:
        out("dry run: OpenBao was not changed and no token was printed")
        return 0
    out("Owner application token (shown once, not stored anywhere else):")
    out(token)
    out("Service token rotated; the previous one stays accepted until finalize.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
