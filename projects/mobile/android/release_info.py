#!/usr/bin/env python3
"""Read or write the metadata of a distributable Home APK.

`_android.yml` builds, tests and signs the APK once, then writes a
`release-info.json` next to it with the fields App Distribution needs later:

    {
      "apk": "app-release.apk",
      "sha256": "…",
      "package": "dev.proflyder.home",
      "firebase_app_id": "1:…:android:…",
      "version_code": 1762700000,
      "version_name": "0.1.0-abc1234",
      "commit": "…",
      "run_url": "…"
    }

The upload job (`android-distribute`) reads it so it needs only the Firebase
service account: it never sees the signing key or `google-services.json`.

Usage:
    release_info.py app-id <google-services.json> [package]
    release_info.py write --apk <apk> --google-services <json>
                          --version-code <int> --version-name <name>
                          --output <release-info.json>
                          [--commit <sha>] [--run-url <url>]
    release_info.py read <release-info.json> <key>
"""
from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys

DEFAULT_PACKAGE = "dev.proflyder.home"


def firebase_app_id(path: str, package: str) -> str:
    with open(path, encoding="utf-8") as handle:
        config = json.load(handle)
    for client in config.get("client", []):
        info = client.get("client_info", {})
        if info.get("android_client_info", {}).get("package_name") == package:
            app_id = info.get("mobilesdk_app_id", "")
            if app_id:
                return app_id
    sys.exit(f"{path} has no client for {package}")


def sha256(path: str) -> str:
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def write(args: argparse.Namespace) -> None:
    for path in (args.apk, args.google_services):
        if not os.path.isfile(path):
            sys.exit(f"missing {path}")
    info = {
        "apk": os.path.basename(args.apk),
        "sha256": sha256(args.apk),
        "package": args.package,
        "firebase_app_id": firebase_app_id(args.google_services, args.package),
        "version_code": args.version_code,
        "version_name": args.version_name,
        "commit": args.commit,
        "run_url": args.run_url,
    }
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(info, handle, indent=2)
        handle.write("\n")


def resign(args: argparse.Namespace) -> None:
    """Point release-info.json at the signed APK (name and digest)."""
    if not os.path.isfile(args.apk):
        sys.exit(f"missing {args.apk}")
    with open(args.input, encoding="utf-8") as handle:
        info = json.load(handle)
    info["apk"] = os.path.basename(args.apk)
    info["sha256"] = sha256(args.apk)
    with open(args.output, "w", encoding="utf-8") as handle:
        json.dump(info, handle, indent=2)
        handle.write("\n")


def read(args: argparse.Namespace) -> None:
    with open(args.file, encoding="utf-8") as handle:
        info = json.load(handle)
    if args.key not in info:
        sys.exit(f"{args.file} has no {args.key}")
    value = info[args.key]
    print("" if value is None else value)


def main(argv: list[str]) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)

    app_id = sub.add_parser("app-id", help="print the Firebase app id")
    app_id.add_argument("google_services")
    app_id.add_argument("package", nargs="?", default=DEFAULT_PACKAGE)

    write_parser = sub.add_parser("write", help="write release-info.json")
    write_parser.add_argument("--apk", required=True)
    write_parser.add_argument("--google-services", required=True)
    write_parser.add_argument("--version-code", type=int, required=True)
    write_parser.add_argument("--version-name", required=True)
    write_parser.add_argument("--output", required=True)
    write_parser.add_argument("--package", default=DEFAULT_PACKAGE)
    write_parser.add_argument("--commit", default="")
    write_parser.add_argument("--run-url", default="")

    resign_parser = sub.add_parser("resign", help="update apk/sha256 for a signed APK")
    resign_parser.add_argument("--input", required=True)
    resign_parser.add_argument("--apk", required=True)
    resign_parser.add_argument("--output", required=True)

    read_parser = sub.add_parser("read", help="print one field")
    read_parser.add_argument("file")
    read_parser.add_argument("key")

    args = parser.parse_args(argv)
    if args.command == "app-id":
        print(firebase_app_id(args.google_services, args.package))
    elif args.command == "write":
        write(args)
    elif args.command == "resign":
        resign(args)
    else:
        read(args)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
