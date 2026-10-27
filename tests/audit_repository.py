#!/usr/bin/env python3
"""Read-only publication checks; print paths/line numbers, never matched values."""
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]


def main():
    entries = subprocess.check_output(
        ["git", "ls-files", "--stage", "-z"], cwd=ROOT
    ).decode().split("\0")
    errors = []
    documents = 0
    tracked = {entry.split("\t", 1)[1] for entry in entries if entry}
    pattern = re.compile(
        r"(?:/Users/stanislav[p]lotnikov|prof[l]yder|R101[C]M3008W60N)|"
        r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|"
        r"\bgh[pousr]_[A-Za-z0-9]{30,}|\bsk-[A-Za-z0-9]{25,}|"
        r"\beyJhbGciOi[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.|"
        r"spotify_refresh_token:\s*[A-Za-z0-9_-]{20,}|"
        r"station_control_bearer:\s*[0-9a-fA-F]{32,}"
    )
    for entry in filter(None, entries):
        meta, name = entry.split("\t", 1)
        path = ROOT / name
        if meta.startswith("160000"):
            errors.append(f"gitlink: {name}")
            continue
        if name.startswith(("secrets/", "backups/", "logs/", "snapshots/")):
            errors.append(f"private tracked path: {name}")
        if path.is_symlink():
            if not path.exists() or "secrets" in path.resolve().relative_to(ROOT).parts:
                errors.append(f"private/broken tracked symlink: {name}")
            continue
        if path.suffix in {".key", ".p12", ".pfx", ".pcap", ".pcapng", ".wav", ".pcm", ".mp3"}:
            errors.append(f"private media/key tracked: {name}")
        if not path.is_file():
            errors.append(f"missing tracked file: {name}")
            continue
        data = path.read_bytes()
        if b"\0" in data:
            continue
        text = data.decode(errors="replace")
        for number, line in enumerate(text.splitlines(), 1):
            if pattern.search(line):
                errors.append(f"credential pattern: {name}:{number}")
        if path.suffix != ".md":
            continue
        documents += 1
        for link in re.findall(r"\[[^\]]*\]\(([^)]+)\)", text):
            target = link.split("#", 1)[0]
            if not target or re.match(r"\w+://|mailto:", target):
                continue
            if target == "url" and name == "docs/HISTORY.md":
                continue  # Literal Markdown syntax example in the historical log.
            if not (path.parent / target).exists():
                errors.append(f"broken local Markdown link: {name}: {target}")
            else:
                import os
                rel = os.path.normpath(str((path.parent / target).relative_to(ROOT)))
                if rel not in tracked and not any(p.startswith(rel.rstrip("/") + "/") for p in tracked):
                    errors.append(f"link requires untracked local data: {name}: {target}")
    for item in errors:
        print(item)
    print(f"Checked {documents} Markdown documents; {len(errors)} issues.")
    return bool(errors)


if __name__ == "__main__":
    sys.exit(main())
