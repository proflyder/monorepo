#!/usr/bin/env python3
"""OpenBao restore drill on a throwaway local stand.

Restores a raft snapshot into a fresh OpenBao container (same pinned image,
same static seal key), checks that it auto-unseals, regenerates a root token
from the recovery keys, inventories mounts/auth/policies/KV paths and compares
every KV entry consumed by External Secrets with the live Kubernetes Secrets
by SHA-256. No secret value is printed; the stand token is revoked and the
container and its data are removed at the end (unless --keep).

The live cluster is only read: the snapshot file, ExternalSecret specs and
the synced Secrets. Taking a fresh snapshot is a separate, documented step:

    kubectl -n openbao create job --from=cronjob/openbao-raft-snapshot <name>

Usage (Docker/Colima on the workstation, SSH host alias from the private
ssh_config):

    ./restore_drill.py --ssh-config secrets/proxmox-install/ssh_config \
        --ssh-host k3s [--snapshot openbao-<ts>.snap | --snapshot-file F]
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
VERSIONS = ROOT / "projects" / "home" / "kubernetes" / "versions.sh"
SECRETS = ROOT / "secrets" / "openbao"
BACKUP_DIR = "/srv/k3s/volumes/openbao-backups"
CONTAINER = "restore-drill-openbao"
PORT = 18200

# Mirrors values/openbao.yaml: raft node id, static seal, declarative audit.
# Unauthenticated generate-root is disabled by default in OpenBao 2.7; the
# stand enables it so the recovery keys can mint a root token.
STAND_CONFIG = """\
ui = false
disable_mlock = true
api_addr = "http://127.0.0.1:8200"
cluster_addr = "http://127.0.0.1:8201"
listener "tcp" {
  address = "0.0.0.0:8200"
  tls_disable = true
  disable_unauthed_generate_root_endpoints = false
}
storage "raft" {
  path    = "/openbao/data"
  node_id = "openbao-0"
}
seal "static" {
  current_key_id = "static-1"
  current_key    = "file:///openbao/seal/unseal.key"
}
audit "file" "stdout" {
  options {
    file_path = "stdout"
  }
}
"""


TEMPLATED = "templated"


def pinned_image(versions: str) -> str:
    tag = re.search(r"^OPENBAO_IMAGE_VERSION=(\S+)", versions, re.M)
    digest = re.search(r"^OPENBAO_IMAGE_DIGEST=(sha256:[0-9a-f]{64})", versions, re.M)
    if not tag or not digest:
        raise SystemExit("OpenBao image pin not found in versions.sh")
    return f"quay.io/openbao/openbao:{tag.group(1)}@{digest.group(1)}"


def decode_otp_token(encoded: str, otp: str) -> str:
    raw = base64.b64decode(encoded + "=" * (-len(encoded) % 4))
    return bytes(a ^ b for a, b in zip(raw, otp.encode())).decode()


def digest(value: str | bytes) -> str:
    if isinstance(value, str):
        value = value.encode()
    return hashlib.sha256(value).hexdigest()


def expected_secret_hashes(external_secrets: list[dict], kv: dict[str, dict]) -> dict:
    """Map (namespace, secret, key) -> sha256 of the value the stand would sync.

    Supports the two shapes used here: dataFrom.extract (all fields) and
    data[].remoteRef with a property. A missing path/field maps to None.
    Templated targets render other keys, so their source fields are only
    checked for presence (TEMPLATED); compare() skips the live value.
    """
    out: dict[tuple[str, str, str], str | None] = {}
    for item in external_secrets:
        ns = item["metadata"]["namespace"]
        spec = item["spec"]
        name = spec.get("target", {}).get("name") or item["metadata"]["name"]
        templated = bool(spec.get("target", {}).get("template"))
        for ref in spec.get("dataFrom", []):
            path = ref.get("extract", {}).get("key")
            if path is None:
                continue
            fields = kv.get(path)
            if fields is None:
                out[(ns, name, f"<extract {path}>")] = None
                continue
            for field, value in fields.items():
                out[(ns, name, field)] = TEMPLATED if templated else digest(str(value))
        for entry in spec.get("data", []):
            remote = entry["remoteRef"]
            value = kv.get(remote["key"], {}).get(remote.get("property", ""))
            if value is None:
                out[(ns, name, entry["secretKey"])] = None
            else:
                out[(ns, name, entry["secretKey"])] = TEMPLATED if templated else digest(str(value))
    return out


def compare(expected: dict, live: dict) -> list[str]:
    problems = []
    for key, want in sorted(expected.items()):
        got = live.get(key)
        label = "/".join(key)
        if want is None:
            problems.append(f"missing in snapshot: {label}")
        elif want == TEMPLATED:
            continue
        elif got is None:
            problems.append(f"missing in live Secret: {label}")
        elif got != want:
            problems.append(f"value differs: {label}")
    return problems


class Stand:
    def __init__(self, token: str = ""):
        self.token = token

    def call(self, path: str, method: str = "GET", body=None, raw: bytes | None = None):
        data = raw if raw is not None else (json.dumps(body).encode() if body is not None else None)
        req = urllib.request.Request(f"http://127.0.0.1:{PORT}/v1/{path}", method=method, data=data)
        if self.token:
            req.add_header("X-Vault-Token", self.token)
        with urllib.request.urlopen(req, timeout=30) as resp:
            text = resp.read()
        return json.loads(text) if text else {}

    def wait(self, predicate, what: str, timeout: int = 60):
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                req = urllib.request.Request(f"http://127.0.0.1:{PORT}/v1/sys/seal-status")
                with urllib.request.urlopen(req, timeout=5) as resp:
                    status = json.load(resp)
                if predicate(status):
                    return status
            except (OSError, json.JSONDecodeError):  # URLError/timeouts while restarting
                pass
            time.sleep(1)
        raise SystemExit(f"stand did not become {what} in {timeout}s")

    def wait_active(self, timeout: int = 60):
        deadline = time.time() + timeout
        while time.time() < deadline:
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{PORT}/v1/sys/health", timeout=5) as r:
                    if r.status == 200:
                        return
            except OSError:  # HTTPError (standby/sealed), URLError, timeouts
                pass
            time.sleep(1)
        raise SystemExit("stand did not become active")

    def kv_dump(self) -> tuple[dict, dict[str, dict]]:
        mounts = self.call("sys/mounts")["data"]
        meta: dict[str, dict] = {}
        values: dict[str, dict] = {}
        for mount, info in mounts.items():
            if info["type"] != "kv":
                continue

            def walk(prefix: str) -> None:
                try:
                    keys = self.call(f"{mount}metadata/{prefix}?list=true")["data"]["keys"]
                except urllib.error.HTTPError as err:
                    if err.code == 404:
                        return
                    raise
                for key in keys:
                    if key.endswith("/"):
                        walk(prefix + key)
                        continue
                    path = prefix + key
                    md = self.call(f"{mount}metadata/{path}")["data"]
                    meta[f"{mount}{path}"] = {
                        "version": md["current_version"],
                        "updated": md["updated_time"][:19],
                    }
                    values[path] = self.call(f"{mount}data/{path}")["data"]["data"]

            walk("")
        return meta, values


def run(cmd: list[str], **kw) -> subprocess.CompletedProcess:
    return subprocess.run(cmd, check=True, **kw)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--ssh-config", required=True)
    parser.add_argument("--ssh-host", default="k3s")
    src = parser.add_mutually_exclusive_group()
    src.add_argument("--snapshot", help=f"file name in {BACKUP_DIR} (default: newest)")
    src.add_argument("--snapshot-file", help="local snapshot file")
    parser.add_argument("--seal-key", default=str(SECRETS / "seal.key"))
    parser.add_argument("--init-json", default=str(SECRETS / "init.json"))
    parser.add_argument("--keep", action="store_true", help="leave the stand running")
    args = parser.parse_args()

    ssh = ["ssh", "-F", args.ssh_config, args.ssh_host]
    kubectl = ssh + ["sudo", "-n", "k3s", "kubectl"]
    image = pinned_image(VERSIONS.read_text())
    # Under the home directory: Colima/Docker Desktop share it with the VM,
    # /var/folders is not mounted. Private and git-ignored.
    base = ROOT / "secrets" / "local" / "restore-drill"
    base.mkdir(parents=True, exist_ok=True, mode=0o700)
    work = Path(tempfile.mkdtemp(prefix="openbao-", dir=base))
    stand = Stand()
    report: dict = {"image": image}
    ok = False
    try:
        for sub in ("config", "seal", "data", "audit"):
            (work / sub).mkdir(mode=0o700)
        (work / "config" / "config.hcl").write_text(STAND_CONFIG)
        shutil.copyfile(args.seal_key, work / "seal" / "unseal.key")

        live_seal = json.loads(run(kubectl + ["-n", "openbao", "get", "secret", "openbao-seal", "-o", "json"],
                                   capture_output=True, text=True).stdout)["data"]["unseal.key"]
        report["seal_key_matches_live"] = digest(base64.b64decode(live_seal)) == digest(
            (work / "seal" / "unseal.key").read_bytes())

        snap = work / "snapshot.snap"
        if args.snapshot_file:
            shutil.copyfile(args.snapshot_file, snap)
            report["snapshot"] = Path(args.snapshot_file).name
        else:
            name = args.snapshot or run(
                ssh + [f"sudo -n ls -1 {BACKUP_DIR} | grep '\\.snap$' | sort | tail -1"],
                capture_output=True, text=True).stdout.strip()
            if not re.fullmatch(r"[\w.-]+\.snap", name):
                raise SystemExit(f"unexpected snapshot name: {name!r}")
            with snap.open("wb") as fh:
                run(ssh + [f"sudo -n cat {BACKUP_DIR}/{name}"], stdout=fh)
            remote_sum = run(ssh + [f"sudo -n sha256sum {BACKUP_DIR}/{name}"],
                             capture_output=True, text=True).stdout.split()[0]
            report["snapshot"] = name
            report["snapshot_sha256_matches"] = remote_sum == digest(snap.read_bytes())

        subprocess.run(["docker", "rm", "-f", CONTAINER], capture_output=True)
        run(["docker", "run", "-d", "--name", CONTAINER, "-p", f"127.0.0.1:{PORT}:8200",
             "-v", f"{work}/config:/openbao/config:ro", "-v", f"{work}/seal:/openbao/seal:ro",
             "-v", f"{work}/data:/openbao/data", "-v", f"{work}/audit:/openbao/audit",
             image, "server", "-config=/openbao/config/config.hcl"], capture_output=True)
        stand.wait(lambda s: s.get("initialized") is False, "reachable")

        # Throwaway init only to obtain a token that can call snapshot-force.
        init = stand.call("sys/init", "PUT", {"recovery_shares": 1, "recovery_threshold": 1})
        stand.token = init["root_token"]
        stand.wait_active()
        stand.call("sys/storage/raft/snapshot-force", "POST", raw=snap.read_bytes())
        stand.token = ""
        status = stand.wait(lambda s: s.get("initialized") and not s.get("sealed"), "unsealed")
        report["auto_unsealed_after_restore"] = True
        report["recovery_shares"] = f"{status['t']}/{status['n']}"

        run(["docker", "restart", CONTAINER], capture_output=True)
        stand.wait(lambda s: s.get("initialized") and not s.get("sealed"), "unsealed after restart")
        stand.wait_active()
        report["auto_unsealed_after_restart"] = True

        attempt = stand.call("sys/generate-root/attempt", "PUT", {})
        keys = json.loads(Path(args.init_json).read_text())["recovery_keys_b64"]
        result = {}
        for key in keys[: attempt["required"]]:
            result = stand.call("sys/generate-root/update", "PUT", {"key": key, "nonce": attempt["nonce"]})
        if not result.get("complete"):
            raise SystemExit("generate-root did not complete with the recovery keys")
        stand.token = decode_otp_token(result["encoded_token"], attempt["otp"])
        report["root_from_recovery_keys"] = True

        report["auth"] = sorted(stand.call("sys/auth")["data"])
        report["policies"] = sorted(stand.call("sys/policies/acl?list=true")["data"]["keys"])
        meta, values = stand.kv_dump()
        report["kv"] = meta

        ess = json.loads(run(kubectl + ["get", "externalsecret", "-A", "-o", "json"],
                             capture_output=True, text=True).stdout)["items"]
        live: dict[tuple[str, str, str], str] = {}
        for ns, name in sorted({(e["metadata"]["namespace"],
                                 e["spec"].get("target", {}).get("name") or e["metadata"]["name"]) for e in ess}):
            proc = subprocess.run(kubectl + ["-n", ns, "get", "secret", name, "-o", "json"],
                                  capture_output=True, text=True)
            if proc.returncode != 0:
                continue
            for field, value in (json.loads(proc.stdout).get("data") or {}).items():
                live[(ns, name, field)] = digest(base64.b64decode(value))
        expected = expected_secret_hashes(ess, values)
        report["external_secret_fields_checked"] = sum(v != TEMPLATED for v in expected.values())
        report["templated_present_only"] = sorted("/".join(k) for k, v in expected.items() if v == TEMPLATED)
        report["differences"] = compare(expected, live)
        ok = (report["seal_key_matches_live"] and report.get("snapshot_sha256_matches", True)
              and not report["differences"])
    finally:
        if stand.token and not args.keep:
            try:
                stand.call("auth/token/revoke-self", "POST", {})
                report["stand_token_revoked"] = True
            except Exception:  # the container is deleted next anyway
                report["stand_token_revoked"] = False
        if not args.keep:
            subprocess.run(["docker", "rm", "-f", CONTAINER], capture_output=True)
            shutil.rmtree(work, ignore_errors=True)
            report["stand_removed"] = True
        else:
            report["stand_workdir"] = str(work)
        print(json.dumps(report, indent=1, ensure_ascii=False))
    return 0 if ok else 1


if __name__ == "__main__":
    sys.exit(main())
