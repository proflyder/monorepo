#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import secrets
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


def issue(bao: ws.Bao, path: str, name: str, token: str, dry_run: bool = False) -> dict:
    values, version = read_secret(bao, path)
    seed = decode_seed(values.get("app_tokens", ""))
    seed = merge_token(seed, name, token)
    values["app_tokens"] = json.dumps(seed, ensure_ascii=False, separators=(",", ":"))
    if not values.get("service_token"):
        values["service_token"] = values.get("token") or new_token()
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
    args = parser.parse_args(argv)
    if not args.name.strip() or "/" in args.name:
        raise SystemExit("name must be a non-empty plain string")
    token = new_token()
    bao = ws.operator(ws.SECRETS)
    try:
        issue(bao, args.path, args.name.strip(), token, args.dry_run)
    except ws.Error as error:
        raise SystemExit(str(error)) from None
    if args.dry_run:
        out("dry run: OpenBao was not changed and no token was printed")
        return 0
    out("Owner application token (shown once, not stored anywhere else):")
    out(token)
    out("Service token updated in OpenBao; restart the workloads to pick it up.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
