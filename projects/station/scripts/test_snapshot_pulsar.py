#!/usr/bin/env python3
"""Behavioural tests for snapshot-pulsar.py using throwaway Git fixtures."""
from __future__ import annotations

import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name('snapshot-pulsar.py')


def run(args: list[str], cwd: Path | None = None) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=cwd, text=True, capture_output=True)


def git(repo: Path, *args: str) -> str:
    result = run(['git', '-C', str(repo), *args])
    if result.returncode != 0:
        raise AssertionError(f"git {' '.join(args)} failed:\n{result.stderr}")
    return result.stdout


def init_repo(path: Path) -> Path:
    path.mkdir(parents=True)
    result = run(['git', '-c', 'init.defaultBranch=main', 'init', '-q', str(path)])
    if result.returncode != 0:
        raise AssertionError(result.stderr)
    git(path, 'config', 'user.email', 'test@example.com')
    git(path, 'config', 'user.name', 'Snapshot Test')
    return path


def commit(repo: Path, message: str) -> str:
    git(repo, 'add', '-A')
    git(repo, 'commit', '-q', '-m', message)
    return git(repo, 'rev-parse', 'HEAD').strip()


def index_state(repo: Path) -> tuple[str, str]:
    return (
        git(repo, 'ls-files', '-s'),
        hashlib.sha256(git(repo, 'diff', '--cached', '--binary').encode()).hexdigest(),
    )


def worktree_state(repo: Path) -> dict[str, str]:
    files: dict[str, str] = {}
    for path in sorted(repo.rglob('*')):
        if '.git' in path.parts or not path.is_file():
            continue
        files[str(path.relative_to(repo))] = hashlib.sha256(path.read_bytes()).hexdigest()
    return files


def state(repo: Path) -> tuple[str, tuple[str, str], dict[str, str]]:
    return (
        git(repo, 'status', '--porcelain=v1', '--untracked-files=all'),
        index_state(repo),
        worktree_state(repo),
    )


def snapshot(root: Path, repo: Path, tag: str = 'v0.5.0') -> tuple[Path, dict]:
    output = root / 'patches'
    result = run([
        sys.executable, str(SCRIPT),
        '--checkout', str(repo), '--output', str(output), '--base', tag,
    ])
    if result.returncode != 0:
        raise AssertionError(result.stderr or result.stdout)
    return output, json.loads((output / 'base.json').read_text())


class SnapshotPulsarTest(unittest.TestCase):
    def test_captures_committed_staged_unstaged_and_untracked(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = init_repo(root / 'upstream')
            (repo / 'tracked.txt').write_text('base\n')
            (repo / 'committed.rs').write_text('pub fn base() {}\n')
            commit(repo, 'upstream base')
            git(repo, 'tag', 'v0.5.0')
            base_commit = git(repo, 'rev-parse', 'HEAD').strip()

            (repo / 'tracked.txt').write_text('committed delta\n')
            (repo / 'committed.rs').write_text('pub fn committed() {}\n')
            commit(repo, 'custom rust work')
            head_commit = git(repo, 'rev-parse', 'HEAD').strip()

            (repo / 'staged.rs').write_text('pub fn staged_v1() {}\n')
            git(repo, 'add', 'staged.rs')
            (repo / 'committed.rs').write_text('pub fn final_worktree() {}\n')
            (repo / 'src').mkdir()
            (repo / 'src' / 'untracked.rs').write_text('pub fn untracked() {}\n')

            before = state(repo)
            output, metadata = snapshot(root, repo)

            self.assertEqual(state(repo), before, 'checkout must stay untouched')
            self.assertEqual(metadata['base_commit'], base_commit)
            self.assertEqual(metadata['head_commit'], head_commit)
            self.assertEqual(metadata['base_ref'], 'v0.5.0')
            self.assertEqual(metadata['branch'], 'main')
            self.assertTrue(metadata['worktree_dirty'])
            self.assertEqual(metadata['new_source_files'], ['src/untracked.rs'])

            patch = (output / 'local.patch').read_text()
            for expected in ('committed.rs', 'staged.rs', 'src/untracked.rs',
                             'committed delta', 'pub fn staged_v1', 'pub fn untracked'):
                self.assertIn(expected, patch)

            restore = root / 'restore'
            result = run(['git', 'clone', '-q', '--no-local', str(repo), str(restore)])
            self.assertEqual(result.returncode, 0, result.stderr)
            git(restore, 'checkout', '-q', base_commit)
            result = run(['git', '-C', str(restore), 'apply', '--binary',
                          str(output / 'local.patch')])
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(worktree_state(restore), worktree_state(repo))

    def test_default_base_uses_pinned_upstream_release(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = init_repo(root / 'upstream')
            (repo / 'base.rs').write_text('pub fn base() {}\n')
            commit(repo, 'upstream base')
            git(repo, 'tag', 'v0.5.2')
            base_commit = git(repo, 'rev-parse', 'HEAD').strip()
            output = root / 'patches'
            result = run([
                sys.executable, str(SCRIPT),
                '--checkout', str(repo), '--output', str(output),
            ])
            self.assertEqual(result.returncode, 0, result.stderr or result.stdout)
            metadata = json.loads((output / 'base.json').read_text())
            self.assertEqual(metadata['base_ref'], 'v0.5.2')
            self.assertEqual(metadata['base_commit'], base_commit)

    def test_clean_committed_delta_is_not_an_empty_patch(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = init_repo(root / 'upstream')
            (repo / 'base.rs').write_text('pub fn base() {}\n')
            commit(repo, 'upstream base')
            git(repo, 'tag', 'v0.5.0')
            (repo / 'ricardo.rs').write_text('pub fn ricardo() {}\n')
            commit(repo, 'committed ricardo work')

            output, metadata = snapshot(root, repo)

            self.assertFalse(metadata['worktree_dirty'])
            patch = (output / 'local.patch').read_text()
            self.assertIn('diff --git a/ricardo.rs b/ricardo.rs', patch)
            self.assertIn('pub fn ricardo', patch)

    def test_refuses_untracked_binary_without_writing_patch(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = init_repo(root / 'upstream')
            (repo / 'base.rs').write_text('pub fn base() {}\n')
            commit(repo, 'upstream base')
            git(repo, 'tag', 'v0.5.0')
            (repo / 'firmware.bin').write_bytes(b'\x7fELF')

            output = root / 'patches'
            result = run([
                sys.executable, str(SCRIPT),
                '--checkout', str(repo), '--output', str(output), '--base', 'v0.5.0',
            ])

            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Review unexpected new asset', result.stderr)
            self.assertFalse((output / 'local.patch').exists())

    def test_refuses_tracked_binary_committed_since_base(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = init_repo(root / 'upstream')
            (repo / 'base.rs').write_text('pub fn base() {}\n')
            commit(repo, 'upstream base')
            git(repo, 'tag', 'v0.5.0')
            (repo / 'firmware.bin').write_bytes(b'\x7fELF')
            commit(repo, 'commit a binary by mistake')

            output = root / 'patches'
            result = run([
                sys.executable, str(SCRIPT),
                '--checkout', str(repo), '--output', str(output), '--base', 'v0.5.0',
            ])

            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Review unexpected new asset', result.stderr)
            self.assertFalse((output / 'local.patch').exists())

    def test_refuses_untracked_credentials(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = init_repo(root / 'upstream')
            (repo / 'base.rs').write_text('pub fn base() {}\n')
            commit(repo, 'upstream base')
            git(repo, 'tag', 'v0.5.0')
            (repo / '.env').write_text('TOKEN=value\n')

            result = run([
                sys.executable, str(SCRIPT),
                '--checkout', str(repo), '--output', str(root / 'patches'),
                '--base', 'v0.5.0',
            ])

            self.assertNotEqual(result.returncode, 0)
            self.assertIn('Review possible secret', result.stderr)

    def test_rejects_base_outside_history(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = init_repo(root / 'upstream')
            (repo / 'base.rs').write_text('pub fn base() {}\n')
            commit(repo, 'upstream base')
            git(repo, 'tag', 'v0.5.0')
            git(repo, 'checkout', '-q', '--orphan', 'unrelated')
            (repo / 'other.rs').write_text('pub fn other() {}\n')
            commit(repo, 'unrelated root')

            result = run([
                sys.executable, str(SCRIPT),
                '--checkout', str(repo), '--output', str(root / 'patches'),
                '--base', 'v0.5.0',
            ])

            self.assertNotEqual(result.returncode, 0)
            self.assertIn('is not an ancestor', result.stderr)


if __name__ == '__main__':
    unittest.main()
