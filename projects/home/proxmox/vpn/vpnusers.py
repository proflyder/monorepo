#!/usr/bin/env python3
"""Add an ocserv user with a strong password and a TOTP secret.

Runs on the workstation. It generates the credentials locally, writes them to
the container through the Proxmox host (`pct push`), and keeps a private copy
plus a Google Authenticator QR code under secrets/home/vpn/.

Secret values are never printed. The script only reports the username and the
paths of the private files.
"""
from __future__ import annotations

import argparse
import base64
import datetime as dt
import os
from pathlib import Path
import re
import secrets
import shlex
import subprocess
import sys
import tempfile

USER_RE = re.compile(r"^[a-z_][a-z0-9_-]{0,31}$")
ISSUER = "home VPN"
ROOT = Path(__file__).resolve().parents[4]


def base32_secret(raw: bytes) -> str:
    return base64.b32encode(raw).decode("ascii").rstrip("=")


def otpauth_url(user: str, secret_b32: str, issuer: str = ISSUER) -> str:
    label = f"{issuer}:{user}"
    query = (
        f"secret={secret_b32}&issuer={issuer.replace(' ', '%20')}"
        "&algorithm=SHA1&digits=6&period=30"
    )
    return f"otpauth://totp/{label.replace(' ', '%20')}?{query}"


def merge_ocpasswd(text: str, user: str, groups: str, password_hash: str) -> str:
    """Replace or append one ocpasswd entry, keeping other users and comments."""
    kept = [
        line
        for line in text.splitlines()
        if line.strip() and not line.startswith(user + ":")
    ]
    kept.append(f"{user}:{groups}:{password_hash}")
    return "\n".join(kept) + "\n"


def merge_otp(text: str, user: str, secret_hex: str) -> str:
    """Replace or append one liboath usersfile entry (HOTP/T30 = TOTP, 30 s)."""
    pattern = re.compile(r"^\S+\s+" + re.escape(user) + r"(?:\s|$)")
    kept = [line for line in text.splitlines() if not pattern.match(line)]
    kept.append(f"HOTP/T30 {user} - {secret_hex}")
    return "\n".join(kept) + "\n"


def password_hash(password: str) -> str:
    result = subprocess.run(
        ["openssl", "passwd", "-6", "-stdin"],
        input=password + "\n",
        text=True,
        capture_output=True,
        check=True,
    )
    return result.stdout.strip()


class Pve:
    def __init__(self, host: str, ctid: int, key: Path | None, known_hosts: Path | None):
        self.host = host
        self.ctid = ctid
        self.ssh = ["ssh", "-o", "BatchMode=yes", "-o", "ConnectTimeout=15"]
        if key:
            self.ssh += ["-i", str(key)]
        if known_hosts:
            self.ssh += ["-o", f"UserKnownHostsFile={known_hosts}"]
        self.ssh.append(f"root@{host}")

    def run(self, command: str, input_text: str | None = None, check: bool = True):
        return subprocess.run(
            self.ssh + [command],
            input=input_text,
            text=True,
            capture_output=True,
            check=check,
        )

    def read_container_file(self, path: str) -> str:
        result = self.run(
            f"pct exec {self.ctid} -- cat {shlex.quote(path)}", check=False
        )
        if result.returncode != 0:
            if "No such file" in result.stderr:
                return ""
            raise RuntimeError(result.stderr.strip())
        return result.stdout

    def write_container_file(self, path: str, content: str, perms: int = 0o600) -> None:
        name = f"/tmp/vpnusers-{secrets.token_hex(8)}"
        try:
            self.run(f"cat > {shlex.quote(name)}", input_text=content)
            self.run(
                f"pct push {self.ctid} {shlex.quote(name)} {shlex.quote(path)} "
                f"--perms {perms:o}"
            )
        finally:
            self.run(f"rm -f {shlex.quote(name)}", check=False)


def write_qr(url: str, path: Path) -> None:
    try:
        import qrcode  # type: ignore
    except ImportError:
        qrcode = None
    if qrcode is not None:
        qrcode.make(url).save(path)
        return
    result = subprocess.run(
        ["qrencode", "-o", str(path)],
        input=url,
        text=True,
        capture_output=True,
    )
    if result.returncode != 0:
        raise RuntimeError("install python qrcode or qrencode to create the QR PNG")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("user", help="VPN username")
    parser.add_argument("--pve", required=True, help="Proxmox host address")
    parser.add_argument("--ctid", type=int, default=102)
    parser.add_argument("--ssh-key", type=Path)
    parser.add_argument("--known-hosts", type=Path)
    parser.add_argument("--groups", default="lan,full", help="ocserv groups")
    parser.add_argument("--issuer", default=ISSUER, help="TOTP issuer label")
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=ROOT / "secrets" / "home" / "vpn",
        help="private directory for the local copy and QR",
    )
    parser.add_argument("--force", action="store_true", help="overwrite local files")
    args = parser.parse_args()

    if not USER_RE.match(args.user):
        parser.error("username must match ^[a-z_][a-z0-9_-]{0,31}$")
    groups = ",".join(part.strip() for part in args.groups.split(",") if part.strip())
    if not groups:
        parser.error("at least one group is required")

    out_dir: Path = args.output_dir
    cred_path = out_dir / f"{args.user}.credentials"
    qr_path = out_dir / f"{args.user}.png"
    if not args.force and (cred_path.exists() or qr_path.exists()):
        parser.error(f"local files already exist for {args.user}; use --force")

    password = secrets.token_urlsafe(24)
    raw_secret = secrets.token_bytes(20)
    secret_b32 = base32_secret(raw_secret)
    secret_hex = raw_secret.hex()
    hashed = password_hash(password)
    url = otpauth_url(args.user, secret_b32, args.issuer)

    pve = Pve(args.pve, args.ctid, args.ssh_key, args.known_hosts)
    passwd = merge_ocpasswd(
        pve.read_container_file("/etc/ocserv/ocpasswd"), args.user, groups, hashed
    )
    otp = merge_otp(pve.read_container_file("/etc/ocserv/users.oath"), args.user, secret_hex)
    pve.write_container_file("/etc/ocserv/ocpasswd", passwd)
    pve.write_container_file("/etc/ocserv/users.oath", otp)

    out_dir.mkdir(parents=True, exist_ok=True, mode=0o700)
    os.chmod(out_dir, 0o700)
    created = dt.datetime.now(dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    cred = (
        f"username={args.user}\n"
        f"password={password}\n"
        f"totp_secret_base32={secret_b32}\n"
        f"otpauth_uri={url}\n"
        f"issuer={args.issuer}\n"
        f"groups={groups}\n"
        f"created={created}\n"
    )
    fd = os.open(cred_path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, "w", encoding="utf-8") as handle:
        handle.write(cred)
    os.chmod(cred_path, 0o600)
    write_qr(url, qr_path)
    os.chmod(qr_path, 0o600)

    print(f"user: {args.user}")
    print(f"groups: {groups}")
    print(f"credentials: {cred_path}")
    print(f"qr: {qr_path}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
