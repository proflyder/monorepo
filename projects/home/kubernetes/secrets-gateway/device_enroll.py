#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import os
import pathlib
import secrets
import sys
import tempfile
import time

ROOT = pathlib.Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / "platform" / "workstation"))

import workstation as ws

PATH = "k8s/home-assistant/secrets-gateway-device"
CODE_FIELD = "enrollment_code_hash"
EXPIRES_FIELD = "enrollment_expires_at"
DEVICE_FIELDS = (
    "device_id",
    "device_public_key",
    "device_enrolled_at",
    "device_attestation_sha256",
    "device_fingerprint",
)
REVOKE_FIELDS = ("device_revoke_device_id", "device_revoked_at")
DEFAULT_OUT = ROOT / "secrets" / "local" / "secrets-gateway" / "device-enroll.txt"
ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"
CODE_BYTES = 16
TTL_SECONDS = 900


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


def normalize_code(value: str) -> str:
    return "".join(
        character for character in value
        if not character.isspace() and character != "-"
    ).upper()


def hash_code(value: str) -> str:
    return hashlib.sha256(normalize_code(value).encode()).hexdigest()


def generate_code() -> str:
    return base32_encode(secrets.token_bytes(CODE_BYTES))


def without_credentials(values: dict[str, str]) -> dict[str, str]:
    removed = set(DEVICE_FIELDS) | set(REVOKE_FIELDS) | {CODE_FIELD, EXPIRES_FIELD}
    return {key: value for key, value in values.items() if key not in removed}


def plan(values: dict[str, str], now: int, revoke: bool) -> tuple[dict[str, str], str | None]:
    previous_device = values.get("device_id")
    updated = without_credentials(values)
    if revoke:
        if previous_device:
            updated["device_revoke_device_id"] = previous_device
            updated["device_revoked_at"] = str(now)
        return updated, None
    code = generate_code()
    updated[CODE_FIELD] = hash_code(code)
    updated[EXPIRES_FIELD] = str(now + TTL_SECONDS)
    return updated, code


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
        description="Create a one-time secrets-gateway device enrollment code")
    parser.add_argument("--revoke", action="store_true",
                        help="remove the enrolled device and any pending code")
    parser.add_argument("--out", default=str(DEFAULT_OUT),
                        help="enrollment code file (0600), not printed")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(sys.argv[1:] if argv is None else argv)
    bao = ws.operator(ROOT / "secrets")
    body = bao.call("GET", f"secret/data/{PATH}", missing_ok=True) or {}
    data = dict((body.get("data") or {}).get("data") or {})
    version = int(((body.get("data") or {}).get("metadata") or {}).get("version", 0))
    updated, code = plan(data, int(time.time()), args.revoke)
    bao.call("POST", f"secret/data/{PATH}",
             {"data": updated, "options": {"cas": version}})
    if code is None:
        print(f"secret/{PATH}: device and pending enrollment removed; "
              "the gateway announces the revocation on its next request")
        return 0
    write_private(pathlib.Path(args.out), code + "\n")
    print(f"secret/{PATH}: enrollment code is in {args.out} (0600, expires in "
          f"{TTL_SECONDS // 60} minutes)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
