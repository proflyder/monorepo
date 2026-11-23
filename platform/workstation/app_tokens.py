#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import pathlib
import secrets
import subprocess
import sys

import workstation as ws

PATH = "k8s/home-assistant/scheduler-auth"
TOKEN_FILE = "secrets/local/app-tokens/owner.token"
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
KUBECTL_NAMESPACE = "home-assistant"
TUNNEL = pathlib.Path(__file__).resolve().parent / "with-kube-tunnel.sh"
DERIVED_SECRETS = {"scheduler-auth", "phone-push", "mcp-gateway-scheduler", "intercom"}


def digest(token: str) -> str:
    return hashlib.sha256(token.encode()).hexdigest()


def new_token() -> str:
    return f"home_{secrets.token_urlsafe(32)}"


def write_owner_file(path: pathlib.Path, token: str) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        handle.write(token + "\n")
    os.chmod(path, 0o600)


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
        if name in DERIVED_SECRETS:
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
            [str(TUNNEL), "kubectl", "-n", KUBECTL_NAMESPACE, "get", "secrets", "-o", "json"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )
    configmaps_doc = json.loads(
        run(
            [str(TUNNEL), "kubectl", "-n", KUBECTL_NAMESPACE, "get", "configmaps", "-o", "json"],
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
    previous = values.get("service_token")
    if not previous:
        raise ValueError("service_token is missing: run admin:service-token-init first")
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


def verify_service_token(bao: ws.Bao, path: str, run=None) -> list[str]:
    if run is None:
        run = subprocess.run
    values, _ = read_secret(bao, path)
    expected = values.get("service_token", "")
    if not expected:
        return ["scheduler-auth: service_token is missing"]
    missing = []
    for name in sorted(DERIVED_SECRETS):
        result = subprocess.run(
            [
                str(TUNNEL),
                "kubectl",
                "-n",
                KUBECTL_NAMESPACE,
                "get",
                "secret",
                name,
                "-o",
                "jsonpath={.data.service_token}",
            ],
            check=False,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            missing.append(f"Secret/{name}: missing")
            continue
        try:
            actual = base64.b64decode(result.stdout.strip()).decode()
        except (ValueError, TypeError):
            actual = ""
        if actual != expected:
            missing.append(f"Secret/{name}:service_token")
    result = subprocess.run(
        [
            str(TUNNEL),
            "kubectl",
            "-n",
            KUBECTL_NAMESPACE,
            "exec",
            "deploy/scheduler",
            "--",
            "/usr/local/bin/scheduler",
            "-service-token-sha256",
        ],
        check=False,
        capture_output=True,
        text=True,
    )
    expected_hash = hashlib.sha256(expected.encode()).hexdigest()
    if result.returncode != 0 or result.stdout.strip() != expected_hash:
        missing.append("Pod/scheduler:service_token")
    return missing


def init_service_token(bao: ws.Bao, path: str, dry_run: bool = False) -> bool:
    values, version = read_secret(bao, path)
    if values.get("service_token"):
        return False
    values["service_token"] = new_token()
    values.setdefault("service_token_previous", "")
    if not dry_run:
        bao.call(
            "POST",
            f"secret/data/{path}",
            {"data": values, "options": {"cas": version}},
        )
    return True


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
    parser.add_argument("--token-file", default=TOKEN_FILE)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--finalize", action="store_true")
    parser.add_argument("--skip-preflight", action="store_true")
    parser.add_argument("--init-service-token", action="store_true")
    parser.add_argument("--verify-service-token", action="store_true")
    args = parser.parse_args(argv)
    if not args.name.strip() or "/" in args.name:
        raise SystemExit("name must be a non-empty plain string")
    bao = ws.operator(ws.SECRETS)
    if args.init_service_token:
        try:
            init_service_token(bao, args.path, args.dry_run)
        except ws.Error as error:
            raise SystemExit(str(error)) from None
        return 0
    if args.verify_service_token:
        try:
            missing = verify_service_token(bao, args.path)
        except ws.Error as error:
            raise SystemExit(str(error)) from None
        if missing:
            raise SystemExit("service token is stale in: " + ", ".join(missing))
        return 0
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
    token_path = pathlib.Path(args.token_file)
    if not args.dry_run:
        write_owner_file(token_path, token)
    try:
        issue(bao, args.path, args.name.strip(), token, args.dry_run)
    except (ws.Error, ValueError) as error:
        raise SystemExit(str(error)) from None
    if args.dry_run:
        out("dry run: OpenBao was not changed and no token was written")
        return 0
    out(f"Owner application token written to {token_path} (mode 0600, shown nowhere else)")
    out("Service token rotated; the previous one stays accepted until finalize.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
