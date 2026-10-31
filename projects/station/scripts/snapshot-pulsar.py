#!/usr/bin/env python3
"""Record the active Rust Pulsar delta in the lab repo, without changing its checkout.

The patch captures everything between an explicit upstream base (default
``v0.5.3``) and the current worktree: committed work, staged and unstaged
edits, and untracked source files. It is intentionally not ``git diff HEAD``,
which would be empty on a clean checkout with committed custom work.
"""
from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
LAB = ROOT.parents[1]
DEFAULT_CHECKOUT = ROOT / 'upstream/pulsar-rust'
DEFAULT_OUTPUT = ROOT / 'patches/pulsar-rust'
DEFAULT_BASE = 'v0.5.3'
MAX_SOURCE_BYTES = 2_000_000
REJECTED_SUFFIXES = {
    '.bin', '.debug', '.zip', '.tar', '.gz', '.tgz', '.xz', '.wav', '.pcm',
    '.mp3', '.flac', '.onnx', '.pem', '.key', '.p12', '.pfx',
}
REJECTED_NAMES = {'.env', 'id_rsa', 'id_ed25519', 'credentials', 'secrets'}


def git(checkout: Path, *args: str) -> bytes:
    result = subprocess.run(
        ['git', '-C', str(checkout), *args],
        stdout=subprocess.PIPE, stderr=subprocess.PIPE,
    )
    if result.returncode != 0:
        raise SystemExit(f"git {' '.join(args)} failed in {checkout}:\n"
                         f"{result.stderr.decode(errors='replace')}")
    return result.stdout


def git_status(checkout: Path, *args: str) -> int:
    return subprocess.run(
        ['git', '-C', str(checkout), *args],
        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
    ).returncode


def display_path(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(LAB))
    except ValueError:
        return str(path.resolve())


def changed_sources(checkout: Path, base_commit: str) -> list[tuple[str, str]]:
    """Paths changed by the patch, including deletes, with their status."""
    raw = git(checkout, 'diff', '--name-status', '-z', '--no-renames',
              base_commit, '--', '.')
    fields = raw.decode().split('\0')
    if fields and fields[-1] == '':
        fields.pop()
    return [(fields[i], fields[i + 1]) for i in range(0, len(fields), 2)]


def check_new_source(name: str, path: Path) -> None:
    """Refuse to silently add binaries, credentials, or oversized assets."""
    parts = set(Path(name).parts)
    if parts & REJECTED_NAMES or path.name in REJECTED_NAMES:
        raise SystemExit(f'Review possible secret before snapshot: {name}')
    if path.suffix.lower() in REJECTED_SUFFIXES:
        raise SystemExit(f'Review unexpected new asset before snapshot: {name}')
    if path.is_symlink():
        return
    if not path.is_file():
        raise SystemExit(f'Review unexpected new path before snapshot: {name}')
    if path.stat().st_size > MAX_SOURCE_BYTES:
        raise SystemExit(f'Review oversized new file before snapshot: {name}')


def untracked_sources(checkout: Path) -> list[str]:
    names = git(checkout, 'ls-files', '--others', '--exclude-standard', '-z')
    return sorted(filter(None, names.decode().split('\0')))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--checkout', default=str(DEFAULT_CHECKOUT),
                        help='Pulsar checkout to snapshot (read-only)')
    parser.add_argument('--output', default=str(DEFAULT_OUTPUT),
                        help='directory for local.patch and base.json')
    parser.add_argument('--base', default=DEFAULT_BASE,
                        help=f'pinned upstream base ref (default: {DEFAULT_BASE})')
    args = parser.parse_args(argv)

    checkout = Path(args.checkout).resolve()
    output = Path(args.output).resolve()
    if not (checkout / '.git').exists():
        raise SystemExit(f'Not a Git checkout: {checkout}')

    base_commit = git(checkout, 'rev-parse', f'{args.base}^{{commit}}').decode().strip()
    head_commit = git(checkout, 'rev-parse', 'HEAD').decode().strip()
    if git_status(checkout, 'merge-base', '--is-ancestor', base_commit, head_commit) != 0:
        raise SystemExit(f'Base {args.base} ({base_commit[:12]}) is not an ancestor of HEAD')

    patch = bytearray(git(
        checkout, 'diff', '--binary', '--no-ext-diff', '--no-textconv',
        base_commit, '--', '.',
    ))
    for status, name in changed_sources(checkout, base_commit):
        if status != 'D':
            check_new_source(name, checkout / name)
    additions: list[str] = []
    for name in untracked_sources(checkout):
        path = checkout / name
        check_new_source(name, path)
        result = subprocess.run(
            ['git', '-C', str(checkout), 'diff', '--no-index', '--binary',
             '--no-ext-diff', '--no-textconv', '--', '/dev/null', name],
            stdout=subprocess.PIPE, stderr=subprocess.PIPE,
        )
        if result.returncode not in (0, 1):
            raise SystemExit(result.stderr.decode(errors='replace'))
        patch.extend(result.stdout)
        additions.append(name)

    output.mkdir(parents=True, exist_ok=True)
    patch_path = output / 'local.patch'
    patch_tmp = output / 'local.patch.tmp'
    patch_tmp.write_bytes(patch)
    os.replace(patch_tmp, patch_path)

    dirty = bool(git(checkout, 'status', '--porcelain=v1').strip())
    metadata = {
        'schema': 2,
        'checkout': display_path(checkout),
        'base_ref': args.base,
        'base_commit': base_commit,
        'base_description': git(checkout, 'describe', '--tags', '--always',
                                base_commit).decode().strip(),
        'head_commit': head_commit,
        'head_description': git(checkout, 'describe', '--tags', '--always',
                                'HEAD').decode().strip(),
        'branch': git(checkout, 'branch', '--show-current').decode().strip(),
        'worktree_dirty': dirty,
        'new_source_files': additions,
        'restore': {
            'checkout': f'git checkout {base_commit}',
            'apply': f'git apply --binary {display_path(patch_path)}',
        },
    }
    base_tmp = output / 'base.json.tmp'
    base_tmp.write_text(json.dumps(metadata, ensure_ascii=False, indent=2) + '\n')
    os.replace(base_tmp, output / 'base.json')

    print(f"Pulsar snapshot: {output / 'local.patch'} "
          f"({len(patch)} bytes, {len(additions)} new files, "
          f"base {base_commit[:12]}, head {head_commit[:12]})")
    return 0


if __name__ == '__main__':
    sys.exit(main())
