#!/usr/bin/env python3
"""Install the pinned Puriso Piper voice; never starts or restarts services."""

import argparse
import hashlib
from pathlib import Path
import urllib.request

REPO = "dhammagift/piper-ru-puriso"
REVISION = "4f826cf39157118a2854f0b825917c1f83a342e7"
FILES = {
    "ru_puriso2-medium.onnx": "7297a037d662068bcd23008df83455a6590891f8538a43dbddd6030592b75f95",
    "ru_puriso2-medium.onnx.json": "af3c323b8895ba6d982cba44dd63cca4ea603ec2c67587168e8ab3d81d9ac3fd",
}


def digest(path):
    checksum = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            checksum.update(chunk)
    return checksum.hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data-dir", type=Path, required=True)
    parser.add_argument("--verify-only", action="store_true")
    args = parser.parse_args()
    if not args.verify_only:
        args.data_dir.mkdir(parents=True, exist_ok=True)
    for filename, expected in FILES.items():
        target = args.data_dir / filename
        if target.exists():
            if digest(target) != expected:
                raise SystemExit(f"Different file already exists, refusing to overwrite: {target}")
        else:
            if args.verify_only:
                raise SystemExit(f"Missing file: {target}")
            partial = target.with_name(target.name + ".part")
            try:
                url = f"https://huggingface.co/{REPO}/resolve/{REVISION}/{filename}"
                with urllib.request.urlopen(url, timeout=90) as response, partial.open("wb") as stream:
                    while chunk := response.read(1024 * 1024):
                        stream.write(chunk)
                if digest(partial) != expected:
                    raise SystemExit(f"Checksum mismatch: {filename}")
                partial.replace(target)
            finally:
                partial.unlink(missing_ok=True)
        print(f"Verified: {filename} ({target.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
