#!/usr/bin/env python3
"""Generate a Yandex Station sound pairing code locally (no Telegram, no cloud).

Usage:
  python3 scripts/station-pairing-sound.py \
    --ssid 'MyWiFi' --password 'secret' --out /tmp/pair.wav
  python3 scripts/station-pairing-sound.py \
    --ssid 'MyWiFi' --password 'secret' --format ogg --out /tmp/pair.ogg

The password is passed on the command line, so use a private terminal/history.
"""
from __future__ import annotations

import argparse
import sys
from pathlib import Path

LAB = Path(__file__).resolve().parents[1]
REPO = LAB / "resources" / "quasar-connect-bot"
sys.path.insert(0, str(REPO))

import pairing  # noqa: E402


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--ssid", required=True)
    ap.add_argument("--password", required=True)
    ap.add_argument("--token", default="", help="xTokenCode; default: random non-empty")
    ap.add_argument("--format", choices=["wav", "ogg"], default="wav")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()

    token = args.token or pairing.random_token()
    params = pairing.PairingParams(
        ssid=args.ssid,
        password=args.password,
        security=pairing.Security.WPA,
        token=token,
    )
    rendered = pairing.render(params)
    data = rendered.wav() if args.format == "wav" else rendered.voice()
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_bytes(data)
    print(f"wrote {out} ({len(data)} bytes, {rendered.duration:.2f}s, {args.format})")
    print("token: " + token if args.token else "token: random non-empty (not printed)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
