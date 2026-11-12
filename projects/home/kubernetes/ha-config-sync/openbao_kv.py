#!/usr/bin/env python3
"""OpenBao KV helper for the ha-config-sync bootstrap.

Only the owner bootstrap uses it. Values are read/written through the
workstation operator token (`secrets/openbao/operator.token`); nothing is
printed except a status word and the non-secret Flux settings on `settings`.

Commands:
  settings              print HAOS_IP and NODE_LAN_IP as JSON
  has FIELD             exit 0 when the field exists and is non-empty
  get-key FILE          write ssh_private_key to FILE (mode 0600)
  put-key KEYFILE KH    merge ssh_private_key/known_hosts with GET->CAS POST
"""
from __future__ import annotations

import json
import os
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / "platform" / "workstation"))

import workstation as ws  # noqa: E402

PATH = "k8s/home-assistant/ha-config-sync"
# The same app settings Flux substitutes into the ha-config-sync manifests.
SETTINGS = "k8s/flux-system/app-settings"


def read(path: str) -> tuple[dict, int]:
    result = ws.operator(ROOT / "secrets").call(
        "GET", f"secret/data/{path}", missing_ok=True
    )
    if not result:
        return {}, 0
    data = result.get("data", {})
    return data.get("data", {}), data.get("metadata", {}).get("version", 0)


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    command = argv[1]
    if command == "settings":
        data, _ = read(SETTINGS)
        print(json.dumps({
            "HAOS_IP": data.get("HAOS_IP", ""),
            "NODE_LAN_IP": data.get("NODE_LAN_IP", ""),
        }))
        return 0
    if command == "has" and len(argv) == 3:
        data, _ = read(PATH)
        return 0 if str(data.get(argv[2], "")).strip() else 1
    if command == "get-key" and len(argv) == 3:
        data, _ = read(PATH)
        value = data.get("ssh_private_key", "")
        if not value:
            print("missing ssh_private_key", file=sys.stderr)
            return 1
        target = pathlib.Path(argv[2])
        target.write_text(value if value.endswith("\n") else value + "\n")
        os.chmod(target, 0o600)
        return 0
    if command == "put-key" and len(argv) == 4:
        key = pathlib.Path(argv[2]).read_text()
        known_hosts = pathlib.Path(argv[3]).read_text()
        current, version = read(PATH)
        merged = dict(current)
        merged["ssh_private_key"] = key if key.endswith("\n") else key + "\n"
        merged["known_hosts"] = (
            known_hosts if known_hosts.endswith("\n") else known_hosts + "\n"
        )
        ws.operator(ROOT / "secrets").call(
            "POST",
            f"secret/data/{PATH}",
            {"data": merged, "options": {"cas": version}},
        )
        print("updated")
        return 0
    print(f"unknown command or arguments: {argv[1:]}", file=sys.stderr)
    return 2


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
