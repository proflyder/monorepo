#!/usr/bin/env python3
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import pathlib
import re
import secrets
import subprocess
import sys
import tempfile
import time

import workstation as ws

PATH = "k8s/home-assistant/scheduler-auth"
TOKEN_FILE = "secrets/local/app-tokens/owner.token"
OWNER_ID = "owner"
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
    "secrets",
    "admin",
]
NAMESPACE = "home-assistant"
TUNNEL = pathlib.Path(__file__).resolve().parent / "with-kube-tunnel.sh"
SCHEDULER = "scheduler"
SERVICE_SECRETS = {
    "scheduler-auth": "service_token",
    "phone-push": "service_token",
    "mcp-gateway-scheduler": "service_token",
    "intercom": "service_token",
}
SERVICE_CONSUMERS = [
    "phone-push",
    "mcp-gateway",
    "intercom",
    "ha-control",
    "ha-bridge",
    "ha-scenario",
    "hermes",
]
HERMES_CONTAINER = "voice-proxy"
HERMES_TOKEN_ENV = "HERMES_SCHEDULER_TOKEN"
SCHEDULER_EXTERNAL_SECRET = (
    pathlib.Path(__file__).resolve().parents[2]
    / "projects/home/kubernetes/scheduler/manifests/external-secrets.yaml"
)
INIT_KEYS = {"service_token", "service_token_previous", "app_tokens"}
EMPTY_SEED = json.dumps({"revision": 0, "tokens": []}, separators=(",", ":"))
SYNC_TIMEOUT = 300.0
POLL_INTERVAL = 3.0


class Failure(SystemExit):
    pass


def digest(token: str) -> str:
    return hashlib.sha256(token.encode()).hexdigest()


def new_token() -> str:
    return f"home_{secrets.token_urlsafe(32)}"


def stage_owner_file(path: pathlib.Path, token: str) -> pathlib.Path:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(token + "\n")
            handle.flush()
            os.fsync(handle.fileno())
    except BaseException:
        pathlib.Path(name).unlink(missing_ok=True)
        raise
    return pathlib.Path(name)


def read_owner_file(path: pathlib.Path) -> str:
    try:
        return path.read_text(encoding="utf-8").strip()
    except FileNotFoundError:
        return ""


def read_secret(bao: ws.Bao, path: str) -> tuple[dict, int]:
    body = bao.call("GET", f"secret/data/{path}")
    data = (body or {}).get("data") or {}
    return dict(data.get("data") or {}), int(data.get("metadata", {}).get("version", 0))


def write_secret(bao: ws.Bao, path: str, values: dict, version: int) -> None:
    bao.call("POST", f"secret/data/{path}", {"data": values, "options": {"cas": version}})


def decode_seed(value: str) -> dict:
    if not value.strip():
        return {"revision": 0, "tokens": []}
    seed = json.loads(value)
    if not isinstance(seed, dict) or not isinstance(seed.get("tokens"), list):
        raise ValueError("app_tokens must be an access snapshot")
    return seed


def owner_entry(seed: dict) -> dict | None:
    return next((item for item in seed.get("tokens", []) if item.get("id") == OWNER_ID), None)


def merge_owner(seed: dict, name: str, token_digest: str) -> dict:
    entry = {
        "id": OWNER_ID,
        "name": name,
        "sha256": token_digest,
        "owner": True,
        "modules": MODULES,
    }
    tokens = [item for item in seed.get("tokens", []) if item.get("id") != OWNER_ID]
    tokens.append(entry)
    return {"revision": 0, "tokens": sorted(tokens, key=lambda item: item.get("id", ""))}


def encode_seed(seed: dict) -> str:
    return json.dumps(seed, ensure_ascii=False, separators=(",", ":"))


def declared_properties(manifest: pathlib.Path = SCHEDULER_EXTERNAL_SECRET) -> set[str]:
    pattern = re.compile(
        r"key:\s*" + re.escape(PATH) + r"\s*\n\s*property:\s*(\S+)"
    )
    return set(pattern.findall(manifest.read_text(encoding="utf-8")))


def init_service_token(bao: ws.Bao, path: str, dry_run: bool = False) -> bool:
    values, version = read_secret(bao, path)
    foreign = sorted(
        key for key in declared_properties() - INIT_KEYS if not str(values.get(key, "")).strip()
    )
    if foreign:
        raise Failure(
            "OpenBao scheduler-auth lacks keys the ExternalSecret needs and init cannot "
            "invent: " + ", ".join(foreign)
        )
    before = dict(values)
    if not values.get("service_token"):
        values["service_token"] = new_token()
    values.setdefault("service_token_previous", "")
    if not str(values.get("app_tokens", "")).strip():
        values["app_tokens"] = EMPTY_SEED
    if values == before:
        return False
    if values["service_token"] == values.get("token"):
        raise Failure("service_token equals the legacy application token")
    if not dry_run:
        write_secret(bao, path, values, version)
    return True


def plan(values: dict, name: str, file_token: str, rotate_owner: bool, rotate_service: bool):
    if not values.get("service_token"):
        raise Failure("service_token is missing: run admin:service-token-init first")
    seed = decode_seed(str(values.get("app_tokens", "")))
    current = owner_entry(seed)
    owner_token = ""
    if current is None or rotate_owner:
        owner_token = new_token()
    elif not file_token or digest(file_token) != current.get("sha256"):
        raise Failure(
            "an owner token is already issued but the local token file does not match it; "
            "pass --rotate-owner to issue a new one (the old owner token stops working)"
        )
    updated = dict(values)
    token_digest = digest(owner_token) if owner_token else current["sha256"]
    updated["app_tokens"] = encode_seed(merge_owner(seed, name, token_digest))
    rotating = bool(values.get("service_token_previous"))
    if rotate_service and not rotating:
        updated["service_token_previous"] = values["service_token"]
        updated["service_token"] = new_token()
        rotating = True
    return updated, owner_token, rotating


class Kube:
    def __init__(self, run=None, sleep=time.sleep, clock=time.monotonic, out=print):
        self.run_process = run or subprocess.run
        self.sleep = sleep
        self.clock = clock
        self.out = out

    def run(self, *args: str, check: bool = True) -> subprocess.CompletedProcess:
        result = self.run_process(
            [str(TUNNEL), "kubectl", "-n", NAMESPACE, *args],
            check=False,
            capture_output=True,
            text=True,
        )
        if check and result.returncode != 0:
            raise Failure(f"kubectl {' '.join(args[:2])} failed: {result.stderr.strip()}")
        return result

    def json(self, *args: str) -> dict:
        return json.loads(self.run(*args, "-o", "json").stdout or "{}")

    def wait(self, what: str, ready, timeout: float = SYNC_TIMEOUT) -> None:
        deadline = self.clock() + timeout
        while not ready():
            if self.clock() >= deadline:
                raise Failure(f"timed out waiting for {what}")
            self.sleep(POLL_INTERVAL)

    def secret_value(self, name: str, key: str) -> str | None:
        result = self.run("get", "secret", name, "-o", "json", check=False)
        if result.returncode != 0:
            return None
        data = (json.loads(result.stdout or "{}").get("data") or {})
        if key not in data:
            return None
        return base64.b64decode(data[key]).decode()

    def refresh_time(self, name: str) -> str:
        status = self.json("get", f"externalsecret/{name}").get("status") or {}
        return str(status.get("refreshTime", ""))

    def externalsecret_ready(self, name: str) -> bool:
        status = self.json("get", f"externalsecret/{name}").get("status") or {}
        return any(
            item.get("type") == "Ready" and item.get("status") == "True"
            for item in status.get("conditions") or []
        )

    def force_sync(self, expected: dict[str, dict[str, str]]) -> None:
        before = {name: self.refresh_time(name) for name in expected}
        stamp = str(int(time.time()))
        for name in expected:
            self.run("annotate", f"externalsecret/{name}", f"force-sync={stamp}", "--overwrite")
        for name, keys in expected.items():
            self.wait(
                f"a new sync of ExternalSecret/{name}",
                lambda name=name: self.refresh_time(name) != before[name]
                and self.externalsecret_ready(name),
            )
            for key, value in keys.items():
                self.wait(
                    f"Secret/{name}:{key} to carry the OpenBao value",
                    lambda name=name, key=key, value=value: self.secret_value(name, key) == value,
                )

    def exists(self, name: str) -> bool:
        return self.run("get", f"deployment/{name}", check=False).returncode == 0

    def restart(self, name: str) -> bool:
        if not self.exists(name):
            self.out(f"deployment/{name} is not deployed here; skipped")
            return False
        self.run("rollout", "restart", f"deployment/{name}")
        self.run("rollout", "status", f"deployment/{name}", "--timeout=300s")
        return True

    def restarted_since(self, name: str, since: str) -> bool:
        result = self.run("get", f"deployment/{name}", "-o", "json", check=False)
        if result.returncode != 0:
            return False
        document = json.loads(result.stdout or "{}")
        annotations = (
            document.get("spec", {}).get("template", {}).get("metadata", {}).get("annotations")
            or {}
        )
        restarted = str(annotations.get("kubectl.kubernetes.io/restartedAt", ""))
        status = document.get("status") or {}
        replicas = int(status.get("replicas", 0))
        settled = (
            int(status.get("observedGeneration", 0))
            >= int(document.get("metadata", {}).get("generation", 0))
            and int(status.get("updatedReplicas", 0)) == replicas
            and int(status.get("readyReplicas", 0)) == replicas
        )
        return bool(restarted) and restarted >= since and settled

    def scheduler_service_hash(self) -> str:
        result = self.run(
            "exec",
            f"deploy/{SCHEDULER}",
            "--",
            "/usr/local/bin/scheduler",
            "-service-token-sha256",
            check=False,
        )
        return result.stdout.strip() if result.returncode == 0 else ""

    def hermes_token_source(self) -> tuple[str, str] | None:
        result = self.run("get", "deployment/hermes", "-o", "json", check=False)
        if result.returncode != 0:
            return None
        containers = (
            json.loads(result.stdout or "{}")
            .get("spec", {})
            .get("template", {})
            .get("spec", {})
            .get("containers", [])
        )
        for container in containers:
            if container.get("name") != HERMES_CONTAINER:
                continue
            for item in container.get("env") or []:
                if item.get("name") == HERMES_TOKEN_ENV:
                    ref = (item.get("valueFrom") or {}).get("secretKeyRef") or {}
                    return str(ref.get("name", "")), str(ref.get("key", ""))
            return ("", "")
        return None


def legacy_consumers(legacy: str, secrets_doc: dict, configmaps_doc: dict) -> list[str]:
    if not legacy:
        return []
    found = []
    for item in secrets_doc.get("items", []):
        name = item.get("metadata", {}).get("name", "")
        if name in SERVICE_SECRETS:
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


def preflight(values: dict, kube: Kube) -> list[str]:
    problems = []
    source = kube.hermes_token_source()
    if source is not None and source != ("scheduler-auth", "service_token"):
        problems.append(
            f"deployment/hermes {HERMES_CONTAINER} {HERMES_TOKEN_ENV} must come from "
            "scheduler-auth#service_token (runtime-values)"
        )
    legacy = values.get("token", "")
    if legacy:
        problems += legacy_consumers(
            legacy, kube.json("get", "secrets"), kube.json("get", "configmaps")
        )
    return problems


def verify(values: dict, kube: Kube, owner_digest: str) -> list[str]:
    problems = []
    for name, key in SERVICE_SECRETS.items():
        if kube.secret_value(name, key) != values["service_token"]:
            problems.append(f"Secret/{name}:{key}")
    if kube.secret_value("scheduler-auth", "service_token_previous") != values.get(
        "service_token_previous", ""
    ):
        problems.append("Secret/scheduler-auth:service_token_previous")
    seed = decode_seed(kube.secret_value("scheduler-auth", "app_tokens") or "")
    entry = owner_entry(seed)
    if not entry or entry.get("sha256") != owner_digest:
        problems.append("Secret/scheduler-auth:app_tokens owner")
    if kube.scheduler_service_hash() != digest(values["service_token"]):
        problems.append(f"Pod/{SCHEDULER}:service_token")
    return problems


def scheduler_expectations(values: dict) -> dict[str, dict[str, str]]:
    return {
        "scheduler-auth": {
            "service_token": values["service_token"],
            "service_token_previous": values.get("service_token_previous", ""),
            "app_tokens": values["app_tokens"],
        }
    }


def service_expectations(values: dict) -> dict[str, dict[str, str]]:
    expected = {name: {key: values["service_token"]} for name, key in SERVICE_SECRETS.items()}
    expected.update(scheduler_expectations(values))
    return expected


def utc_now() -> str:
    return time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())


def require_clean(problems: list[str], stage: str) -> None:
    if problems:
        raise Failure(f"{stage}: stale or missing: " + ", ".join(problems))


def bootstrap(
    bao: ws.Bao,
    kube: Kube,
    path: str,
    name: str,
    token_file: pathlib.Path,
    rotate_owner: bool = False,
    rotate_service: bool = False,
    skip_preflight: bool = False,
    dry_run: bool = False,
    out=print,
) -> None:
    values, version = read_secret(bao, path)
    if not skip_preflight:
        problems = preflight(values, kube)
        if problems:
            raise Failure(
                "preflight failed; nothing was changed: " + ", ".join(problems)
                + " (fix the consumers or pass --skip-preflight at your own risk)"
            )
    updated, owner_token, rotating = plan(
        values, name, read_owner_file(token_file), rotate_owner, rotate_service
    )
    if dry_run:
        out("dry run: OpenBao, Kubernetes and the token file were not changed")
        return
    staged = stage_owner_file(token_file, owner_token) if owner_token else None
    try:
        if updated != values:
            write_secret(bao, path, updated, version)
    except BaseException:
        if staged:
            staged.unlink(missing_ok=True)
        raise
    if staged:
        os.replace(staged, token_file)
        out(f"Owner application token written to {token_file} (mode 0600, shown nowhere else)")
    else:
        out("Owner application token is unchanged")
    owner_digest = owner_entry(decode_seed(updated["app_tokens"]))["sha256"]
    rotation_started = utc_now()
    kube.force_sync(service_expectations(updated) if rotating else scheduler_expectations(updated))
    kube.restart(SCHEDULER)
    if rotating:
        for consumer in SERVICE_CONSUMERS:
            kube.restart(consumer)
    problems = verify(updated, kube, owner_digest)
    if rotating:
        problems += [
            f"deployment/{consumer} was not restarted onto the new service token"
            for consumer in SERVICE_CONSUMERS
            if not kube.restarted_since(consumer, rotation_started)
        ]
    require_clean(problems, "verify")
    if not rotating:
        out("Bootstrap complete; the service token was not rotated")
        return
    final, final_version = read_secret(bao, path)
    final["service_token_previous"] = ""
    write_secret(bao, path, final, final_version)
    kube.force_sync(scheduler_expectations(final))
    kube.restart(SCHEDULER)
    require_clean(verify(final, kube, owner_digest), "finalize")
    out("Service token rotated; the previous one is no longer accepted")


def main(argv: list[str] | None = None, out=print, kube: Kube | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Issue the owner application token and optionally rotate the service token"
    )
    parser.add_argument("--name", default=NAME)
    parser.add_argument("--path", default=PATH)
    parser.add_argument("--token-file", default=TOKEN_FILE)
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--rotate-owner", action="store_true")
    parser.add_argument("--rotate-service", action="store_true")
    parser.add_argument("--skip-preflight", action="store_true")
    parser.add_argument("--init-service-token", action="store_true")
    args = parser.parse_args(argv)
    name = args.name.strip()
    if not name or len(name) > 64 or any(not char.isprintable() for char in name):
        raise SystemExit("name must be a printable string of 1-64 characters")
    bao = ws.operator(ws.SECRETS)
    try:
        if args.init_service_token:
            init_service_token(bao, args.path, args.dry_run)
            return 0
        bootstrap(
            bao,
            kube or Kube(out=out),
            args.path,
            name,
            pathlib.Path(args.token_file),
            rotate_owner=args.rotate_owner,
            rotate_service=args.rotate_service,
            skip_preflight=args.skip_preflight,
            dry_run=args.dry_run,
            out=out,
        )
    except ValueError as error:
        raise SystemExit(str(error)) from None
    return 0


if __name__ == "__main__":
    sys.exit(main())
