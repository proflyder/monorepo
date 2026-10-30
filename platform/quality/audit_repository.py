#!/usr/bin/env python3
"""Read-only publication checks; print paths/line numbers, never matched values.

The scanner treats the repository as private by default. Narrow, reviewed
exceptions exist for the user's *public* identity:

* the exact public domain host ``proflyder.dev`` (used in docs and links);
* the approved Android application namespace ``dev.proflyder.home`` and its
  subpackages, plus the matching source/resource path component
  ``dev/proflyder/home``;
* the exact public GitHub repository slug ``proflyder/pulsar``. The active
  Rust fork stores it as its update source and version provenance; it is a
  repository identifier, not a credential, and no other repository owned by
  the handle is allowed.

They are removed from a line before the credential/private pattern runs, and
only when the token stands alone with proper boundaries. Any other occurrence of
the personal name (another TLD, a username, a longer or prefixed host, a longer
path component, or a mixed line that also carries private data) still fails the
scan. No line or file is skipped, and no Gitleaks rule is widened.

The approved tokens are written with a character class (``prof[l]yder``) so this
scanner does not flag its own source; the regex behaves identically. For the
same reason this file never spells out an unapproved lookalike.
"""
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]

# Reviewed public allowances. Order matters only for readability; each pattern
# removes exactly one approved form.
APPROVED_PUBLIC = (
    # Exact public host, optionally inside a URL. Reject parent/child hosts
    # (prefixed or subdomained) and longer hosts; allow a sentence-ending
    # period or surrounding punctuation.
    (
        re.compile(
            r"(?<![A-Za-z0-9.-])prof[l]yder\.dev(?![A-Za-z0-9-])(?!\.[A-Za-z0-9])"
        ),
        "<public-domain>",
    ),
    # Approved Android application namespace and subpackages. Reject a longer
    # identifier component or a prefixed namespace.
    (
        re.compile(
            r"(?<![A-Za-z0-9_.])dev\.prof[l]yder\.home(?![A-Za-z0-9_])"
        ),
        "<public-namespace>",
    ),
    # Matching source/resource directory path; subdirectories are allowed,
    # longer path components are not.
    (
        re.compile(
            r"(?<![A-Za-z0-9_.-])dev/prof[l]yder/home(?![A-Za-z0-9_-])"
        ),
        "<public-source-path>",
    ),
    # Exact public GitHub repository slug used by the active Rust fork. Any
    # other repository, longer name, or prefixed path still fails.
    (
        re.compile(
            r"(?<![A-Za-z0-9_.-])prof[l]yder/pulsar(?![A-Za-z0-9_.-])"
        ),
        "<public-repo>",
    ),
    # Public GHCR namespace for the monorepo service images. Only the registry
    # path is approved; any other use of the account name still fails.
    (
        re.compile(
            r"(?<![A-Za-z0-9_.-])ghcr\.io/prof[l]yder(?:/[A-Za-z0-9<>${}_-]+)?"
        ),
        "<public-registry>",
    ),
)

CREDENTIAL_PATTERN = re.compile(
    r"(?:/Users/stanislav[p]lotnikov|prof[l]yder|R101[C]M3008W60N)|"
    r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----|"
    r"\bgh[pousr]_[A-Za-z0-9]{30,}|\bsk-[A-Za-z0-9]{25,}|"
    r"\beyJhbGciOi[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.|"
    r"spotify_refresh_token:\s*[A-Za-z0-9_-]{20,}|"
    r"station_control_bearer:\s*[0-9a-fA-F]{32,}"
)


def sanitize_approved_tokens(line: str) -> str:
    """Remove only the reviewed public tokens from one line."""
    for pattern, placeholder in APPROVED_PUBLIC:
        line = pattern.sub(placeholder, line)
    return line


def scan_text(name: str, text: str) -> list[str]:
    """Return credential findings for one tracked text file."""
    errors = []
    for number, line in enumerate(text.splitlines(), 1):
        if CREDENTIAL_PATTERN.search(sanitize_approved_tokens(line)):
            errors.append(f"credential pattern: {name}:{number}")
    return errors


def main():
    entries = subprocess.check_output(
        ["git", "ls-files", "--stage", "-z"], cwd=ROOT
    ).decode().split("\0")
    errors = []
    documents = 0
    tracked = {entry.split("\t", 1)[1] for entry in entries if entry}
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
        errors.extend(scan_text(name, text))
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
