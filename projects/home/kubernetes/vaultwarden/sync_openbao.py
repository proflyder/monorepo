#!/usr/bin/env python3
"""Copy every OpenBao KV secret into Vaultwarden (one-way, idempotent).

    BW_SESSION=... mise run vault:sync [-- --dry-run]

Layout in the vault (folder ``OpenBao``):
  * one Secure Note per KV path, named ``openbao:<path>`` (e.g.
    ``openbao:infra/ssh/proxmox-admin``); short values are hidden custom
    fields, long ones (> FIELD_LIMIT chars: p12, archives, kubeconfigs) are
    attachments named after the field;
  * one SSH key item per private key found in ``secret/infra/ssh/*``
    (``ssh:<path>/<field>``), usable by the Bitwarden SSH agent;
  * ``openbao:bootstrap``: operator token, recovery keys, seal key and
    username/TOTP id from ``secrets/openbao/`` (what OpenBao itself needs).

Every managed item carries a hidden ``_digest`` field (sha256 of its content):
unchanged items are skipped, changed ones are rewritten in place. Items in the
folder that no longer exist in OpenBao are reported, never deleted.

Auth: BW_SESSION from ``bw unlock`` (the CLI must already point at the
Vaultwarden server and be logged in); OpenBao: BAO_TOKEN or
``secrets/openbao/operator.token``. Values never reach stdout or argv: JSON
goes to ``bw`` on stdin, attachments through a private temp directory.
"""
from __future__ import annotations

import argparse
import base64
import hashlib
import json
import os
import pathlib
import subprocess
import sys
import tempfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parents[4]
SECRETS = ROOT / "secrets"
FOLDER = "OpenBao"
FIELD_LIMIT = 1000
ROOTS = ("ci/", "k8s/", "infra/")
HIDDEN = 1


# --------------------------------------------------------------------------
# OpenBao
# --------------------------------------------------------------------------
class Bao:
    def __init__(self, addr: str, token: str):
        self.addr, self.token = addr.rstrip("/"), token

    def _get(self, method: str, path: str) -> dict | None:
        req = urllib.request.Request(f"{self.addr}/v1/{path}", method=method,
                                     headers={"X-Vault-Token": self.token})
        try:
            with urllib.request.urlopen(req, timeout=20) as resp:
                return json.load(resp)
        except urllib.error.HTTPError as error:
            if error.code == 404:
                return None
            raise

    def walk(self, prefix: str):
        listing = self._get("LIST", f"secret/metadata/{prefix}") or {}
        for key in listing.get("data", {}).get("keys", []):
            if key.endswith("/"):
                yield from self.walk(prefix + key)
            else:
                yield prefix + key

    def read(self, path: str) -> dict[str, str]:
        data = (self._get("GET", f"secret/data/{path}") or {}).get("data", {})
        return {k: str(v) for k, v in (data.get("data") or {}).items()}

    def snapshot(self) -> dict[str, dict[str, str]]:
        return {path: self.read(path) for root in ROOTS for path in self.walk(root)}


# --------------------------------------------------------------------------
# Desired vault content
# --------------------------------------------------------------------------
def digest(payload) -> str:
    return hashlib.sha256(json.dumps(payload, sort_keys=True).encode()).hexdigest()


def note(name: str, values: dict[str, str], description: str) -> dict:
    """A Secure Note spec: short values as fields, long ones as attachments."""
    fields = {k: v for k, v in values.items() if len(v) <= FIELD_LIMIT}
    attachments = {k: v for k, v in values.items() if len(v) > FIELD_LIMIT}
    return {"kind": "note", "name": name, "notes": description,
            "fields": fields, "attachments": attachments,
            "digest": digest([name, description, values])}


def public_and_fingerprint(private: str, public: str) -> tuple[str, str]:
    """Derive the public key when absent and its SHA256 fingerprint (ssh-keygen)."""
    if not public:
        with tempfile.TemporaryDirectory(prefix="bw-ssh-") as tmp:
            key = pathlib.Path(tmp) / "key"
            fd = os.open(key, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, "w") as handle:
                handle.write(private)
            public = subprocess.run(["ssh-keygen", "-y", "-f", str(key)], check=True,
                                    capture_output=True, text=True).stdout.strip()
    out = subprocess.run(["ssh-keygen", "-l", "-E", "sha256", "-f", "-"], input=public + "\n",
                         check=True, capture_output=True, text=True).stdout.split()
    return public, out[1]


def ssh_keys(path: str, values: dict[str, str]) -> list[dict]:
    specs = []
    for field, value in sorted(values.items()):
        if "PRIVATE KEY-----" not in value:
            continue
        public = next((values[c] for c in (f"{field}_pub", f"{field}.pub")
                       if c in values), "")
        private = value.strip() + "\n"
        public, fingerprint = public_and_fingerprint(private, public.strip())
        specs.append({"kind": "ssh", "name": f"ssh:{path}/{field}",
                      "private": private, "public": public, "fingerprint": fingerprint,
                      "notes": f"OpenBao secret/{path}#{field}",
                      "digest": digest([path, field, value, public])})
    return specs


def bootstrap(secrets_dir: pathlib.Path, bao_addr: str) -> dict | None:
    base = secrets_dir / "openbao"
    # Recovery keys and the seal key now live only with the owner. Without them
    # on disk the item is left alone, never rewritten without the keys.
    if not ((base / "init.json").is_file() or (base / "seal.key").is_file()):
        return None
    values: dict[str, str] = {"bao_addr": bao_addr}
    for name in ("operator.token", "username", "totp-method-id"):
        path = base / name
        if path.is_file():
            values[name.replace(".", "_").replace("-", "_")] = path.read_text().strip()
    seal = base / "seal.key"
    if seal.is_file():
        values["seal_key_b64"] = base64.b64encode(seal.read_bytes()).decode()
    init = base / "init.json"
    if init.is_file():
        data = json.loads(init.read_text())
        for index, key in enumerate(data.get("recovery_keys_b64") or [], start=1):
            values[f"recovery_key_{index}_b64"] = key
        values["init_json"] = init.read_text()
    return note("openbao:bootstrap", values,
                "OpenBao bootstrap (outside OpenBao on purpose): operator token "
                "(policy operator-kv), static seal key (base64 of 32 bytes; "
                "Secret openbao/openbao-seal), recovery keys for "
                "`bao operator generate-root`. Source: secrets/openbao/.")


def desired(snapshot: dict[str, dict[str, str]], boot: dict | None) -> list[dict]:
    specs = []
    for path, values in sorted(snapshot.items()):
        specs.append(note(f"openbao:{path}", values,
                          f"OpenBao KV v2: secret/{path}\nSynced by "
                          "projects/home/kubernetes/vaultwarden/sync_openbao.py; "
                          "edit in OpenBao, not here."))
        if path.startswith("infra/ssh/"):
            specs.extend(ssh_keys(path, values))
    if boot:
        specs.append(boot)
    return specs


# --------------------------------------------------------------------------
# Vaultwarden through the bw CLI
# --------------------------------------------------------------------------
class BwError(RuntimeError):
    pass


class Bw:
    def __init__(self, binary: str = "bw"):
        self.binary = binary

    def run(self, *args: str, stdin: str | None = None) -> str:
        result = subprocess.run([self.binary, *args, "--nointeraction"],
                                input=stdin, capture_output=True, text=True)
        if result.returncode != 0:
            # bw errors name the failing object, not values.
            raise BwError(f"bw {args[0]} {args[1] if len(args) > 1 else ''} failed: "
                          f"{result.stderr.strip()[:300]}")
        return result.stdout

    def json(self, *args: str, stdin: str | None = None):
        out = self.run(*args, stdin=stdin)
        return json.loads(out) if out.strip() else None

    def encoded(self, obj: dict) -> str:
        return base64.b64encode(json.dumps(obj).encode()).decode()


def field_list(spec: dict) -> list[dict]:
    fields = [{"name": k, "value": v, "type": HIDDEN, "linkedId": None}
              for k, v in sorted(spec.get("fields", {}).items())]
    fields.append({"name": "_digest", "value": spec["digest"], "type": 0, "linkedId": None})
    return fields


def item_body(spec: dict, folder_id: str, current: dict | None = None) -> dict:
    body = dict(current or {})
    body.update({"organizationId": None, "folderId": folder_id, "name": spec["name"],
                 "notes": spec["notes"], "favorite": False, "reprompt": 0,
                 "fields": field_list(spec)})
    if spec["kind"] == "ssh":
        body.update({"type": 5, "sshKey": {"privateKey": spec["private"],
                                           "publicKey": spec["public"],
                                           "keyFingerprint": spec["fingerprint"]}})
    else:
        body.update({"type": 2, "secureNote": {"type": 0}})
    return body


def current_digest(item: dict) -> str | None:
    for field in item.get("fields") or []:
        if field.get("name") == "_digest":
            return field.get("value")
    return None


def sync(bw: Bw, specs: list[dict], dry_run: bool, log=print) -> dict:
    bw.run("sync")
    folders = bw.json("list", "folders") or []
    folder = next((f for f in folders if f["name"] == FOLDER), None)
    if folder is None:
        if dry_run:
            folder = {"id": "dry-run"}
        else:
            folder = bw.json("create", "folder", stdin=bw.encoded({"name": FOLDER}))
        log(f"folder {FOLDER}: created")
    items = [] if folder["id"] == "dry-run" else (
        bw.json("list", "items", "--folderid", folder["id"]) or [])
    by_name = {item["name"]: item for item in items}
    stats = {"created": 0, "updated": 0, "unchanged": 0, "orphaned": 0}

    for spec in specs:
        item = by_name.get(spec["name"])
        if item and current_digest(item) == spec["digest"]:
            stats["unchanged"] += 1
            continue
        action = "updated" if item else "created"
        stats[action] += 1
        log(f"{action}: {spec['name']}" + (
            f" (+{len(spec.get('attachments', {}))} attachments)" if spec.get("attachments") else ""))
        if dry_run:
            continue
        body = item_body(spec, folder["id"], item)
        if item:
            saved = bw.json("edit", "item", item["id"], stdin=bw.encoded(body))
            for attachment in item.get("attachments") or []:
                bw.run("delete", "attachment", attachment["id"], "--itemid", item["id"])
        else:
            saved = bw.json("create", "item", stdin=bw.encoded(body))
        upload_attachments(bw, saved["id"], spec.get("attachments", {}))

    wanted = {spec["name"] for spec in specs}
    for name in sorted(set(by_name) - wanted):
        stats["orphaned"] += 1
        log(f"not in OpenBao any more (kept): {name}")
    return stats


def upload_attachments(bw: Bw, item_id: str, attachments: dict[str, str]) -> None:
    if not attachments:
        return
    with tempfile.TemporaryDirectory(prefix="bw-attach-") as tmp:
        os.chmod(tmp, 0o700)
        for name, value in sorted(attachments.items()):
            path = pathlib.Path(tmp) / f"{name}.txt"
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            with os.fdopen(fd, "w") as handle:
                handle.write(value)
            bw.run("create", "attachment", "--file", str(path), "--itemid", item_id)
            path.unlink()


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--dry-run", action="store_true", help="report, change nothing")
    ap.add_argument("--secrets-dir", type=pathlib.Path, default=SECRETS)
    ap.add_argument("--bao-public-addr", default=None,
                    help="address recorded in the bootstrap item (default BAO_ADDR)")
    ap.add_argument("--no-bootstrap", action="store_true",
                    help="skip secrets/openbao/ (bootstrap item)")
    args = ap.parse_args(argv)
    if not os.environ.get("BW_SESSION"):
        ap.error("BW_SESSION is not set: run `bw unlock` first")
    sys.path.insert(0, str(ROOT / "platform" / "workstation"))
    import workstation  # same address/token resolution as `mise run secrets:*`
    addr = workstation.address(args.secrets_dir) or ap.error(
        "no OpenBao address: run `mise run secrets:login`")
    token = workstation.token(args.secrets_dir)
    state = workstation.state_dir(args.secrets_dir)
    snapshot = Bao(addr, token).snapshot()
    boot = None if args.no_bootstrap else bootstrap(state, args.bao_public_addr or addr)
    stats = sync(Bw(), desired(snapshot, boot), args.dry_run)
    print(("dry run: " if args.dry_run else "") + ", ".join(f"{k} {v}" for k, v in stats.items()))
    return 0


if __name__ == "__main__":
    sys.exit(main())
