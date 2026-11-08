#!/usr/bin/env python3
"""Workstation access to OpenBao — the only source of secrets.

Run through mise (see mise.toml), e.g. ``mise run secrets:file home/ingress.env``:

  login            human login (userpass + TOTP) → per-machine operator token
  pull             SSH keys/config into ~/.ssh/home-lab, private notes into secrets/
  file NAME...     restore legacy secrets/<NAME> files (0600); --all, dir prefixes
  clean NAME...    remove them again (--all)
  check            compare catalog.json with the paths that exist in OpenBao
  doctor           tools, OpenBao token, SSH to the home nodes, gh login

Local state (git-ignored): secrets/openbao/{addr,operator.token,username}. In a
linked worktree the main checkout's state is used; restored files still land in
this checkout's secrets/. BAO_ADDR / BAO_TOKEN override the state. Values never
go to stdout or argv.
"""
from __future__ import annotations

import argparse
import base64
import getpass
import gzip
import io
import json
import os
import pathlib
import shutil
import socket
import subprocess
import sys
import tarfile
import urllib.error
import urllib.request

HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
SECRETS = ROOT / "secrets"
CATALOG = HERE / "catalog.json"
OPERATOR_POLICY = "operator-kv"
TOKEN_PERIOD = "720h"
ROOTS = ("ci/", "k8s/", "infra/")


class Error(SystemExit):
    pass


def main_checkout(root: pathlib.Path = ROOT) -> pathlib.Path | None:
    try:
        common = subprocess.run(
            ["git", "-C", str(root), "rev-parse", "--path-format=absolute", "--git-common-dir"],
            capture_output=True, text=True, check=True).stdout.strip()
    except (OSError, subprocess.CalledProcessError):
        return None
    return pathlib.Path(common).parent if common else None


def state_dir(secrets_dir: pathlib.Path) -> pathlib.Path:
    """secrets/ holding openbao/{addr,operator.token}: this checkout, else the main one."""
    if (secrets_dir / "openbao").is_dir():
        return secrets_dir
    main = main_checkout()
    if main and (main / "secrets" / "openbao").is_dir():
        return main / "secrets"
    return secrets_dir


# --------------------------------------------------------------------------
# Catalog and encodings
# --------------------------------------------------------------------------
def load_catalog(path: pathlib.Path = CATALOG) -> dict:
    return json.loads(path.read_text())


def decode(value: str, encoding: str) -> bytes:
    """Rebuild original file bytes from a KV string value."""
    raw = value.encode()
    if encoding == "text":
        return raw
    if encoding == "text+nl":
        return raw + b"\n"
    if encoding == "b64":
        return base64.b64decode(value)
    if encoding == "to_b64":
        return base64.b64encode(raw)
    if encoding == "to_b64+nl":
        return base64.b64encode(raw) + b"\n"
    if encoding == "gzip_b64":
        return gzip.decompress(base64.b64decode(value))
    raise ValueError(f"unknown encoding {encoding!r}")


def select(files: list[dict], names: list[str], everything: bool) -> list[dict]:
    if everything:
        return list(files)
    chosen = []
    for name in names:
        name = name.removeprefix("secrets/").rstrip("/")
        hits = [e for e in files if e["file"] == name or e["file"].startswith(name + "/")]
        if not hits:
            raise Error(f"not in catalog: {name} (see platform/workstation/catalog.json)")
        chosen.extend(hits)
    return chosen


def safe_target(base: pathlib.Path, rel: str) -> pathlib.Path:
    target = (base / rel).resolve()
    if base.resolve() not in target.parents:
        raise Error(f"refusing path outside {base}: {rel}")
    return target


def write_private(path: pathlib.Path, data: bytes) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "wb") as handle:
        handle.write(data)
    os.chmod(path, 0o600)


def extract_tar(data: bytes, dest_parent: pathlib.Path) -> None:
    dest_parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    with tarfile.open(fileobj=io.BytesIO(data), mode="r:gz") as archive:
        archive.extractall(dest_parent, filter="data")


def prune_empty(base: pathlib.Path, start: pathlib.Path) -> None:
    """Remove now-empty parent directories up to (not including) base."""
    base = base.resolve()
    current = start
    while current != base and base in current.parents:
        try:
            current.rmdir()
        except OSError:
            return
        current = current.parent


# --------------------------------------------------------------------------
# OpenBao
# --------------------------------------------------------------------------
class Bao:
    def __init__(self, addr: str, token: str | None = None):
        self.addr, self.token = addr.rstrip("/"), token
        self.cache: dict[str, dict] = {}

    def call(self, method: str, path: str, body: dict | None = None,
             missing_ok: bool = False) -> dict | None:
        headers = {"Content-Type": "application/json"}
        if self.token:
            headers["X-Vault-Token"] = self.token
        data = json.dumps(body).encode() if body is not None else None
        req = urllib.request.Request(f"{self.addr}/v1/{path}", data=data,
                                     method=method, headers=headers)
        try:
            with urllib.request.urlopen(req, timeout=20) as resp:
                text = resp.read()
                return json.loads(text) if text else {}
        except urllib.error.HTTPError as error:
            if missing_ok and error.code == 404:
                return None
            raise Error(f"OpenBao {method} {path}: HTTP {error.code}") from None
        except urllib.error.URLError as error:
            raise Error(f"OpenBao unreachable at {self.addr}: {error.reason} "
                        "(LAN or VPN needed)") from None

    def data(self, path: str) -> dict[str, str]:
        if path not in self.cache:
            body = self.call("GET", f"secret/data/{path}", missing_ok=True)
            self.cache[path] = ((body or {}).get("data") or {}).get("data") or {}
        return self.cache[path]

    def field(self, path: str, field: str) -> str:
        values = self.data(path)
        if field not in values:
            raise Error(f"OpenBao secret/{path} has no field {field}")
        return values[field]

    def walk(self, prefix: str):
        body = self.call("LIST", f"secret/metadata/{prefix}", missing_ok=True) or {}
        for key in body.get("data", {}).get("keys", []):
            if key.endswith("/"):
                yield from self.walk(prefix + key)
            else:
                yield prefix + key

    def renew(self) -> dict:
        return self.call("POST", "auth/token/renew-self", {})


def address(secrets_dir: pathlib.Path) -> str | None:
    if os.environ.get("BAO_ADDR"):
        return os.environ["BAO_ADDR"]
    path = state_dir(secrets_dir) / "openbao" / "addr"
    return path.read_text().strip() if path.is_file() else None


def token(secrets_dir: pathlib.Path) -> str:
    if os.environ.get("BAO_TOKEN"):
        return os.environ["BAO_TOKEN"]
    path = state_dir(secrets_dir) / "openbao" / "operator.token"
    if not path.is_file():
        raise Error("no operator token: run `mise run secrets:login` first")
    return path.read_text().strip()


def operator(secrets_dir: pathlib.Path, renew: bool = True) -> Bao:
    addr = address(secrets_dir)
    if not addr:
        raise Error("no OpenBao address: run `mise run secrets:login` first")
    bao = Bao(addr, token(secrets_dir))
    if renew:
        bao.renew()  # periodic token: regular use keeps it alive
    return bao


# --------------------------------------------------------------------------
# Commands
# --------------------------------------------------------------------------
def cmd_login(args, out=print, ask=input, secret=getpass.getpass) -> int:
    addr = args.addr or address(args.secrets_dir) or ask(
        "OpenBao address (https://vault.<home-domain>): ").strip()
    base = state_dir(args.secrets_dir)
    user = args.user or _read(base / "openbao" / "username") or ask("OpenBao user: ").strip()
    human = Bao(addr)
    auth = human.call("POST", f"auth/userpass/login/{user}",
                      {"password": secret(f"OpenBao password for {user}: ")})["auth"]
    if auth.get("mfa_requirement"):
        requirement = auth["mfa_requirement"]
        method_ids = [m["id"] for constraint in requirement["mfa_constraints"].values()
                      for m in constraint["any"]]
        code = secret("TOTP code: ").strip()
        auth = human.call("POST", "sys/mfa/validate", {
            "mfa_request_id": requirement["mfa_request_id"],
            "mfa_payload": {method_ids[0]: [code]}})["auth"]
    human.token = auth["client_token"]
    try:
        label = args.label or socket.gethostname().split(".")[0]
        machine = human.call("POST", "auth/token/create-orphan", {
            "policies": [OPERATOR_POLICY], "period": TOKEN_PERIOD, "renewable": True,
            "no_default_policy": False, "display_name": f"workstation-{label}",
            "meta": {"workstation": label}})["auth"]["client_token"]
    finally:
        human.call("POST", "auth/token/revoke-self", {})
    old = base / "openbao" / "operator.token"
    if old.is_file() and args.revoke_previous:
        try:
            Bao(addr, old.read_text().strip()).call("POST", "auth/token/revoke-self", {})
            out("previous operator token revoked")
        except Error:
            out("previous operator token was already invalid")
    write_private(base / "openbao" / "addr", (addr + "\n").encode())
    write_private(base / "openbao" / "username", (user + "\n").encode())
    write_private(old, (machine + "\n").encode())
    out(f"operator token for workstation-{label} written (policy {OPERATOR_POLICY}, "
        f"period {TOKEN_PERIOD}); human session revoked")
    return 0


def _read(path: pathlib.Path) -> str | None:
    return path.read_text().strip() if path.is_file() else None


def resolve_dest(dest: str, home: pathlib.Path, repo: pathlib.Path = ROOT) -> pathlib.Path:
    if dest.startswith("~/"):
        return home / dest[2:]
    return repo / dest


def cmd_pull(args, out=print) -> int:
    bao = operator(args.secrets_dir)
    home = pathlib.Path(os.environ.get("HOME", str(pathlib.Path.home())))
    changed = 0
    for entry in load_catalog(args.catalog)["pull"]:
        data = bao.field(entry["path"], entry["field"]).encode()
        target = resolve_dest(entry["dest"], home, state_dir(args.secrets_dir).parent)
        if target.is_file() and target.read_bytes() == data:
            continue
        changed += 1
        if args.dry_run:
            out(f"would write {entry['dest']}")
            continue
        if target.is_file():
            shutil.copy2(target, target.with_name(target.name + ".prev"))
        write_private(target, data)
        out(f"wrote {entry['dest']} <- {entry['path']}#{entry['field']}")
    out(f"pull: {changed} changed" + (" (dry run)" if args.dry_run else ""))
    return 0


def cmd_file(args, out=print) -> int:
    entries = select(load_catalog(args.catalog)["files"], args.names, args.all)
    if args.clean:
        for entry in entries:
            target = safe_target(args.secrets_dir, entry["file"])
            if entry["encoding"] == "tar_b64":
                shutil.rmtree(target, ignore_errors=True)
            else:
                target.unlink(missing_ok=True)
            prune_empty(args.secrets_dir, target.parent)
            out(f"removed {entry['file']}")
        return 0
    bao = operator(args.secrets_dir)
    for entry in entries:
        value = bao.field(entry["path"], entry["field"])
        target = safe_target(args.secrets_dir, entry["file"])
        if entry["encoding"] == "tar_b64":
            extract_tar(base64.b64decode(value), target.parent)
        else:
            write_private(target, decode(value, entry["encoding"]))
        out(f"wrote secrets/{entry['file']} <- {entry['path']}#{entry['field']}")
    return 0


FNOX = ROOT / "fnox.toml"


def fnox_refs(path: pathlib.Path = FNOX) -> list[dict]:
    """Every secret reference in fnox.toml as {profile, var, path, field}."""
    import tomllib
    config = tomllib.loads(path.read_text())
    providers = {name: p["path"].removeprefix("secret/") for name, p in config["providers"].items()}
    refs = []
    sections = [("default", config.get("secrets", {}))] + [
        (name, profile.get("secrets", {})) for name, profile in config.get("profiles", {}).items()]
    for profile, secrets in sections:
        for var, spec in secrets.items():
            leaf, field = spec["value"].split("/", 1)
            refs.append({"profile": profile, "var": var,
                         "path": f"{providers[spec['provider']]}/{leaf}", "field": field})
    return refs


def cmd_token(args, out=print) -> int:
    """Print the operator token for fnox (`credential_command`); renews it first.

    Only fnox calls this: the value goes to its stdout pipe, never to a terminal
    or log of ours.
    """
    bao = operator(args.secrets_dir)
    out(bao.token)
    return 0


def cmd_check(args, out=print) -> int:
    catalog = load_catalog(args.catalog)
    bao = operator(args.secrets_dir)
    live = {path for root in ROOTS for path in bao.walk(root)}
    documented = set(catalog["paths"])
    problems = [f"in OpenBao, missing in catalog: {p}" for p in sorted(live - documented)]
    problems += [f"in catalog, missing in OpenBao: {p}" for p in sorted(documented - live)]
    fnox = [dict(r) for r in fnox_refs()] if FNOX.is_file() else []
    for entry in catalog["pull"] + catalog["files"] + fnox:
        if entry["path"] not in documented:
            problems.append(f"undocumented path used: {entry['path']}")
        elif entry["path"] in live and entry["field"] not in bao.data(entry["path"]):
            problems.append(f"missing field: {entry['path']}#{entry['field']}")
    for problem in problems:
        out(problem)
    out(f"check: {len(live)} paths in OpenBao, {len(documented)} in catalog, "
        f"{len(problems)} problems")
    return 1 if problems else 0


def cmd_doctor(args, out=print, run=subprocess.run) -> int:
    failures = 0

    def report(ok: bool, what: str, hint: str = "") -> None:
        nonlocal failures
        failures += 0 if ok else 1
        out(("ok    " if ok else "FAIL  ") + what + ("" if ok or not hint else f" — {hint}"))

    for tool in ("bazelisk", "gitleaks", "kubectl", "helm", "gh", "bao", "vault", "fnox", "bw",
                 "python3", "node"):
        report(shutil.which(tool) is not None, f"tool {tool}", "run `mise install`")
    try:
        bao = operator(args.secrets_dir, renew=False)
        info = bao.renew()["auth"]
        report(OPERATOR_POLICY in info.get("policies", []),
               f"OpenBao token ({info.get('lease_duration', 0) // 3600} h left after renew)")
    except Error as error:
        report(False, "OpenBao token", str(error))
    config = pathlib.Path.home() / ".ssh" / "home-lab" / "config"
    for host in ("pve", "k3s", "haos"):
        ok = config.is_file() and run(
            ["ssh", "-F", str(config), "-o", "ConnectTimeout=6", host, "true"],
            capture_output=True).returncode == 0
        report(ok, f"ssh {host}", "run `mise run secrets:pull` (LAN or VPN needed)")
    gh = run(["gh", "api", "user", "--jq", ".login"], capture_output=True, text=True) \
        if shutil.which("gh") else None
    report(bool(gh and gh.returncode == 0), "gh login", "run `gh auth login`")
    out("doctor: " + ("all good" if not failures else f"{failures} problem(s)"))
    return 1 if failures else 0


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--secrets-dir", type=pathlib.Path, default=SECRETS)
    ap.add_argument("--catalog", type=pathlib.Path, default=CATALOG)
    sub = ap.add_subparsers(dest="cmd", required=True)
    login = sub.add_parser("login")
    login.add_argument("--addr")
    login.add_argument("--user")
    login.add_argument("--label", help="workstation name in the token (default: hostname)")
    login.add_argument("--keep-previous", dest="revoke_previous", action="store_false",
                       help="do not revoke the token this machine had before")
    pull = sub.add_parser("pull")
    pull.add_argument("--dry-run", action="store_true")
    for name in ("file", "clean"):
        p = sub.add_parser(name)
        p.add_argument("names", nargs="*")
        p.add_argument("--all", action="store_true")
    sub.add_parser("check")
    sub.add_parser("token", help="print the operator token (fnox credential_command)")
    sub.add_parser("doctor")
    args = ap.parse_args(argv)
    if args.cmd in ("file", "clean"):
        if not args.names and not args.all:
            ap.error("name files or pass --all")
        args.clean = args.cmd == "clean"
        return cmd_file(args)
    return {"login": cmd_login, "pull": cmd_pull, "check": cmd_check,
            "doctor": cmd_doctor, "token": cmd_token}[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
