#!/usr/bin/env python3
"""Idempotent OpenBao configuration for the home deployment.

Enables the KV v2 mount, the file audit device, the auth methods (userpass for
the human, JWT for GitHub Actions, Kubernetes for pods), the policies and the
non-secret test entries. It never prints tokens or passwords.

Run through bootstrap.sh, or manually with an admin token:

    BAO_TOKEN=... ./configure.py --addr https://vault.<domain> \
        --resolve-ip 192.0.2.27 [--enforce-totp]

--resolve-ip pins the connection to the node while DNS is still propagating.
"""
from __future__ import annotations

import argparse
import json
import os
import secrets
import string
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
SECRETS = ROOT / "secrets" / "openbao"

GITHUB_ISSUER = "https://token.actions.githubusercontent.com"
GITHUB_ENVIRONMENT = "Home"
GITHUB_REF = "refs/heads/develop"
GITHUB_STATION_ENVIRONMENT = "Station"

ADMIN_POLICY = """\
# Full administration, minus operations that can destroy availability or
# bypass the audit trail. Those stay with recovery keys / the node admin.
path "*" {
  capabilities = ["create", "read", "update", "delete", "list", "sudo"]
}

path "sys/seal" {
  capabilities = ["deny"]
}
path "sys/step-down" {
  capabilities = ["deny"]
}
path "sys/raw/*" {
  capabilities = ["deny"]
}
path "sys/replication/*" {
  capabilities = ["deny"]
}
path "sys/rotate" {
  capabilities = ["deny"]
}
path "sys/rekey/*" {
  capabilities = ["deny"]
}
path "sys/rekey-recovery-key/*" {
  capabilities = ["deny"]
}
"""

CI_POLICY = """\
# GitHub Actions may only read the CI entries that deploy-home and the smoke
# check actually use. A new secret/ci/<name> needs an explicit entry here.
path "secret/data/ci/test" {
  capabilities = ["read"]
}
path "secret/data/ci/deploy-home" {
  capabilities = ["read"]
}
path "secret/metadata/ci" {
  capabilities = ["list"]
}
path "secret/metadata/ci/test" {
  capabilities = ["read"]
}
path "secret/metadata/ci/deploy-home" {
  capabilities = ["read"]
}
"""

STATION_POLICY = """\
# The station (firmware) deploy reads exactly its own CI entry.
path "secret/data/ci/deploy-station" {
  capabilities = ["read"]
}
path "secret/metadata/ci/deploy-station" {
  capabilities = ["read"]
}
"""

K8S_POLICY = """\
# Workloads and External Secrets Operator read only secret/k8s/*.
path "secret/data/k8s/*" {
  capabilities = ["read"]
}
path "secret/metadata/k8s/*" {
  capabilities = ["read", "list"]
}
path "secret/metadata/k8s" {
  capabilities = ["list"]
}
"""

BACKUP_POLICY = """\
# Only raft snapshots for the nightly CronJob.
path "sys/storage/raft/snapshot" {
  capabilities = ["read"]
}
"""


class Bao:
    def __init__(self, addr: str, token: str, resolve_ip: str | None) -> None:
        self.addr = addr.rstrip("/")
        self.token = token
        self.resolve_ip = resolve_ip

    def call(self, method: str, path: str, data: dict | None = None) -> tuple[int, object]:
        host = self.addr.split("://", 1)[1].split("/", 1)[0]
        command = ["curl", "-sS", "--max-time", "20", "-X", method]
        if self.resolve_ip:
            command += ["--resolve", f"{host}:443:{self.resolve_ip}"]
        if self.token:
            command += ["-H", "X-Vault-Token: " + self.token]
        if data is not None:
            command += ["-H", "Content-Type: application/json",
                        "--data-binary", json.dumps(data)]
        command += ["-w", "\n%{http_code}", self.addr + path]
        result = subprocess.run(command, capture_output=True, text=True)
        if result.returncode != 0:
            sys.exit(f"curl failed for {method} {path}: {result.stderr.strip()}")
        body, _, code = result.stdout.rpartition("\n")
        parsed: object = None
        if body.strip():
            try:
                parsed = json.loads(body)
            except json.JSONDecodeError:
                parsed = body
        return int(code), parsed

    def expect(self, method: str, path: str, data: dict | None = None) -> object:
        code, body = self.call(method, path, data)
        if not 200 <= code < 300:
            sys.exit(f"{method} {path} failed with {code}: {body}")
        return body


def password(length: int = 28) -> str:
    alphabet = string.ascii_letters + string.digits + "-_"
    return "".join(secrets.choice(alphabet) for _ in range(length))


def keys_of(bao: Bao, path: str) -> dict:
    code, body = bao.call("GET", path)
    if code == 404:
        return {}
    if code != 200 or not isinstance(body, dict):
        sys.exit(f"GET {path} failed with {code}: {body}")
    return body.get("data") or {}


def ensure_kv(bao: Bao) -> None:
    mounts = keys_of(bao, "/v1/sys/mounts")
    if "secret/" not in mounts:
        bao.expect("POST", "/v1/sys/mounts/secret",
                   {"type": "kv", "options": {"version": "2"},
                    "description": "home secrets (KV v2)"})
        print("kv: mounted secret/ (v2)")
        return
    version = (mounts["secret/"].get("options") or {}).get("version")
    if version != "2":
        sys.exit(f"secret/ exists but is not KV v2 (version={version})")
    print("kv: secret/ already present")


def ensure_audit(bao: Bao) -> None:
    # Enabled declaratively in the server config (OpenBao 2.7 rejects API
    # enablement); this only verifies that the device is live.
    devices = keys_of(bao, "/v1/sys/audit")
    if "file/" in devices:
        print("audit: file device active")
    else:
        print("warning: file audit device is not active yet "
              "(declarative config applies on the next server start)",
              file=sys.stderr)


def ensure_auth(bao: Bao, path: str, body: dict) -> bool:
    methods = keys_of(bao, "/v1/sys/auth")
    if path + "/" in methods:
        return False
    bao.expect("POST", f"/v1/sys/auth/{path}", body)
    return True


def ensure_policy(bao: Bao, name: str, policy: str) -> None:
    bao.expect("PUT", f"/v1/sys/policies/acl/{name}", {"policy": policy})
    print(f"policy: {name}")


def ensure_userpass(bao: Bao, user: str) -> None:
    created = ensure_auth(bao, "userpass", {"type": "userpass"})
    print("auth: userpass " + ("enabled" if created else "already enabled"))

    password_file = SECRETS / "human.pass"
    code, _ = bao.call("GET", f"/v1/auth/userpass/users/{user}")
    if code == 200:
        print(f"user: {user} already exists (password unchanged)")
        if not password_file.exists():
            print(f"note: {password_file} is missing; rotate the password in the UI",
                  file=sys.stderr)
        return
    if code != 404:
        sys.exit(f"GET userpass user failed with {code}")

    SECRETS.mkdir(mode=0o700, parents=True, exist_ok=True)
    if password_file.exists():
        secret = password_file.read_text(encoding="utf-8").strip()
    else:
        secret = password()
    bao.expect("POST", f"/v1/auth/userpass/users/{user}",
               {"password": secret, "token_policies": "admin",
                "token_ttl": "8h", "token_max_ttl": "24h"})
    password_file.write_text(secret + "\n", encoding="utf-8")
    password_file.chmod(0o600)
    print(f"user: {user} created (password in {password_file})")


def ensure_github(bao: Bao, audience: str, repository: str,
                  station_repository: str | None = None) -> None:
    created = ensure_auth(bao, "github", {"type": "jwt"})
    print("auth: jwt (github) " + ("enabled" if created else "already enabled"))
    bao.expect("POST", "/v1/auth/github/config",
               {"oidc_discovery_url": GITHUB_ISSUER, "bound_issuer": GITHUB_ISSUER})
    bao.expect("POST", "/v1/auth/github/role/deploy-home", {
        "role_type": "jwt",
        "user_claim": "job_workflow_ref",
        "bound_audiences": [audience],
        "bound_claims": {
            "repository": repository,
            "environment": GITHUB_ENVIRONMENT,
            "ref": GITHUB_REF,
        },
        "token_policies": ["ci-deploy"],
        "token_ttl": "10m",
        "token_max_ttl": "15m",
        "clock_skew": 60,
    })
    print("role: auth/github/role/deploy-home (audience %s)" % audience)
    if not station_repository:
        return
    # The station deploy runs from a tag through a reusable workflow, so it
    # binds repository + environment only: the GitHub Environment restricts
    # which refs may deploy. No ref claim here on purpose.
    bao.expect("POST", "/v1/auth/github/role/deploy-station", {
        "role_type": "jwt",
        "user_claim": "job_workflow_ref",
        "bound_audiences": [audience],
        "bound_claims": {
            "repository": station_repository,
            "environment": GITHUB_STATION_ENVIRONMENT,
        },
        "token_policies": ["ci-station"],
        "token_ttl": "10m",
        "token_max_ttl": "15m",
        "clock_skew": 60,
    })
    print("role: auth/github/role/deploy-station (audience %s)" % audience)


def ensure_kubernetes(bao: Bao) -> None:
    created = ensure_auth(bao, "kubernetes", {"type": "kubernetes"})
    print("auth: kubernetes " + ("enabled" if created else "already enabled"))
    bao.expect("POST", "/v1/auth/kubernetes/config",
               {"kubernetes_host": "https://kubernetes.default.svc:443"})
    roles = {
        "external-secrets": (["external-secrets"], ["external-secrets"], ["k8s-read"]),
        "k8s-smoke": (["openbao-smoke"], ["openbao-smoke"], ["k8s-read"]),
        "backup": (["openbao-backup"], ["openbao"], ["backup"]),
    }
    for name, (accounts, namespaces, policies) in roles.items():
        bao.expect("POST", f"/v1/auth/kubernetes/role/{name}", {
            "bound_service_account_names": accounts,
            "bound_service_account_namespaces": namespaces,
            "token_policies": policies,
            "token_ttl": "20m",
            "token_max_ttl": "1h",
            # Empty: accept the pod's default projected token audience.
            "audience": "",
        })
        print(f"role: auth/kubernetes/role/{name}")


def ensure_secret(bao: Bao, path: str, data: dict) -> None:
    code, _ = bao.call("GET", f"/v1/secret/data/{path}")
    if code == 200:
        print(f"kv: secret/{path} exists")
        return
    if code != 404:
        sys.exit(f"GET secret/{path} failed with {code}")
    bao.expect("POST", f"/v1/secret/data/{path}", {"data": data})
    print(f"kv: secret/{path} seeded")


def ensure_totp(bao: Bao, enforce: bool) -> None:
    code, body = bao.call("LIST", "/v1/identity/mfa/method")
    if code not in (200, 404):
        sys.exit(f"LIST identity/mfa/method failed with {code}: {body}")
    methods = ((body or {}).get("data") or {}).get("keys", []) if code == 200 else []
    method_id = None
    for entry in methods:
        code, body = bao.call("GET", f"/v1/identity/mfa/method/{entry}")
        data = (body or {}).get("data", {}) if isinstance(body, dict) else {}
        if (data.get("name") or data.get("method_name")) == "human-totp":
            method_id = data.get("id", entry)
            break
    if method_id is None:
        body = bao.expect("POST", "/v1/identity/mfa/method/totp", {
            "method_name": "human-totp",
            "issuer": "OpenBao home",
            "period": 30,
            "digits": 6,
            "algorithm": "SHA1",
            "generate": False,
        })
        method_id = (body or {}).get("data", {}).get("method_id")
        print("mfa: method human-totp created (per-entity TOTP)")
    else:
        print("mfa: method human-totp already exists")
    if method_id:
        (SECRETS / "totp-method-id").write_text(method_id + "\n", encoding="utf-8")

    if not enforce:
        # Never removes an existing enforcement; only reports the live state.
        code, _ = bao.call("GET", "/v1/identity/mfa/login-enforcement/userpass-totp")
        if code == 200:
            print("mfa: TOTP already enforced on auth/userpass")
        else:
            print("mfa: enforcement not enabled (enroll a device, then run --enforce-totp)")
        return
    methods_list = keys_of(bao, "/v1/sys/auth")
    accessor = (methods_list.get("userpass/") or {}).get("accessor")
    if not accessor:
        sys.exit("userpass mount accessor is missing")
    bao.expect("POST", "/v1/identity/mfa/login-enforcement/userpass-totp", {
        "mfa_method_ids": [method_id],
        "auth_method_accessors": [accessor],
    })
    print("mfa: TOTP enforced on auth/userpass")


def default_repository() -> str:
    """Owner/name of the GitHub repository, from the env or the git remote."""
    explicit = os.environ.get("OPENBAO_GITHUB_REPOSITORY")
    if explicit:
        return explicit.strip()
    try:
        url = subprocess.run(
            ["git", "config", "--get", "remote.origin.url"],
            capture_output=True, text=True, check=True,
            cwd=ROOT).stdout.strip()
    except (subprocess.CalledProcessError, FileNotFoundError):
        return ""
    if url.endswith(".git"):
        url = url[:-4]
    if url.startswith("git@"):
        _, _, path = url.partition(":")
        return path
    if "://" in url:
        return "/".join(url.split("://", 1)[1].split("/")[1:])
    return url


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--addr", default=os.environ.get("BAO_ADDR"))
    parser.add_argument("--resolve-ip", default=os.environ.get("OPENBAO_NODE_IP"))
    parser.add_argument("--token", default=os.environ.get("BAO_TOKEN"))
    parser.add_argument("--user", default=os.environ.get("OPENBAO_USER"),
                        help="human userpass login (also OPENBAO_USER)")
    parser.add_argument("--repository",
                        default=default_repository(),
                        help="GitHub owner/name for the OIDC role claim")
    parser.add_argument("--audience", default=os.environ.get("OPENBAO_AUDIENCE"),
                        help="GitHub OIDC audience; defaults to --addr")
    parser.add_argument("--station-repository",
                        default=os.environ.get("OPENBAO_STATION_REPOSITORY"),
                        help="GitHub owner/name of the station repository; "
                             "enables the deploy-station role and ci-station "
                             "policy (also OPENBAO_STATION_REPOSITORY)")
    parser.add_argument("--enforce-totp", action="store_true",
                        help="enforce TOTP for userpass (only after enrolment)")
    args = parser.parse_args()
    if not args.addr or not args.token:
        sys.exit("--addr and BAO_TOKEN (or --token) are required")
    if not args.user:
        sys.exit("--user (or OPENBAO_USER) is required for the human login")
    if not args.repository:
        sys.exit("cannot determine the GitHub repository; set "
                 "OPENBAO_GITHUB_REPOSITORY")

    # The audience must be the public URL GitHub requests tokens for, not a
    # temporary port-forward or loopback address used to reach the API.
    audience = (args.audience or args.addr).rstrip("/")
    host = audience.split("://", 1)[-1].split("/", 1)[0].split(":", 1)[0]
    if not audience.startswith("https://") or host in ("127.0.0.1", "localhost"):
        sys.exit("--audience must be the public https URL of OpenBao "
                 "(set OPENBAO_AUDIENCE when --addr is a local forward)")
    bao = Bao(args.addr, args.token, args.resolve_ip)
    station_repository = (args.station_repository or "").strip() or None

    ensure_kv(bao)
    ensure_audit(bao)
    ensure_policy(bao, "admin", ADMIN_POLICY)
    ensure_policy(bao, "ci-deploy", CI_POLICY)
    if station_repository:
        ensure_policy(bao, "ci-station", STATION_POLICY)
    ensure_policy(bao, "k8s-read", K8S_POLICY)
    ensure_policy(bao, "backup", BACKUP_POLICY)
    ensure_userpass(bao, args.user)
    ensure_github(bao, audience, args.repository, station_repository)
    if not station_repository:
        print("station: no --station-repository, deploy-station role skipped")
    ensure_kubernetes(bao)
    ensure_secret(bao, "ci/test", {"probe": "ok-ci",
                                   "purpose": "CI auth smoke test; non-secret"})
    ensure_secret(bao, "k8s/test", {"probe": "ok-k8s",
                                    "purpose": "Kubernetes auth smoke test; non-secret"})
    ensure_totp(bao, args.enforce_totp)
    print("configuration complete")


if __name__ == "__main__":
    main()
