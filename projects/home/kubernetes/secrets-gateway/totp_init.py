#!/usr/bin/env python3
from __future__ import annotations

import argparse
import os
import pathlib
import secrets
import sys
import tempfile
import urllib.parse

ROOT = pathlib.Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / "platform" / "workstation"))

import workstation as ws

PATH = "k8s/home-assistant/secrets-gateway"
FIELD = "totp_secret"
DEFAULT_OUT = ROOT / "secrets" / "local" / "secrets-gateway" / "totp-otpauth.txt"
ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
SECRET_BYTES = 20


def base32_encode(data: bytes) -> str:
    value = 0
    bits = 0
    out: list[str] = []
    for byte in data:
        value = (value << 8) | byte
        bits += 8
        while bits >= 5:
            bits -= 5
            out.append(ALPHABET[(value >> bits) & 31])
    if bits:
        out.append(ALPHABET[(value << (5 - bits)) & 31])
    return "".join(out)


def generate_secret(length: int = SECRET_BYTES) -> str:
    return base32_encode(secrets.token_bytes(length))


def otpauth_uri(secret: str, issuer: str, account: str) -> str:
    label = (urllib.parse.quote(issuer, safe="")
             + ":" + urllib.parse.quote(account, safe=""))
    query = urllib.parse.urlencode({
        "secret": secret,
        "issuer": issuer,
        "algorithm": "SHA1",
        "digits": 6,
        "period": 30,
    })
    return f"otpauth://totp/{label}?{query}"


def plan(values: dict, rotate: bool) -> tuple[dict, str | None]:
    if str(values.get(FIELD, "")).strip() and not rotate:
        return dict(values), None
    secret = generate_secret()
    updated = dict(values)
    updated[FIELD] = secret
    return updated, secret


def write_private(path: pathlib.Path, text: str) -> None:
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(path.parent, 0o700)
    fd, name = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        os.fchmod(fd, 0o600)
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(text)
            handle.flush()
            os.fsync(handle.fileno())
        os.replace(name, path)
    except BaseException:
        pathlib.Path(name).unlink(missing_ok=True)
        raise
    os.chmod(path, 0o600)


def parse_args(argv: list[str]) -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Create the secrets-gateway TOTP secret in OpenBao")
    parser.add_argument("--rotate", action="store_true",
                        help="replace an existing totp_secret")
    parser.add_argument("--out", default=str(DEFAULT_OUT),
                        help="otpauth URI file (0600), not printed")
    parser.add_argument("--issuer", default="Home")
    parser.add_argument("--account", default="owner")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    bao = ws.operator(ROOT / "secrets")
    body = bao.call("GET", f"secret/data/{PATH}", missing_ok=True) or {}
    data = dict((body.get("data") or {}).get("data") or {})
    version = int(((body.get("data") or {}).get("metadata") or {}).get("version", 0))
    updated, secret = plan(data, args.rotate)
    if secret is None:
        print(f"secret/{PATH}#{FIELD} already exists; pass --rotate to replace it")
        return 0
    bao.call("POST", f"secret/data/{PATH}",
             {"data": updated, "options": {"cas": version}})
    uri = otpauth_uri(secret, args.issuer, args.account)
    write_private(pathlib.Path(args.out), uri + "\n")
    print(f"secret/{PATH}#{FIELD} written; otpauth URI is in {args.out} (0600)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
