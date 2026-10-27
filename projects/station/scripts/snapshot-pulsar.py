#!/usr/bin/env python3
"""Record the local Pulsar patch in the lab repo, without changing its checkout."""
from pathlib import Path
import json
import subprocess

ROOT = Path(__file__).resolve().parents[1]
PULSAR = ROOT / 'upstream/pulsar'
OUT = ROOT / 'patches/pulsar'

def git(*args):
    return subprocess.check_output(['git', '-C', str(PULSAR), *args])

def main():
    OUT.mkdir(parents=True, exist_ok=True)
    base = git('rev-parse', 'HEAD').decode().strip()
    patch = bytearray(git('diff', '--binary', '--no-ext-diff', '--no-textconv', 'HEAD', '--', '.'))
    additions = []
    names = git('ls-files', '--others', '--exclude-standard', '-z').decode().split('\0')
    for name in sorted(filter(None, names)):
        path = PULSAR / name
        # Downloaded installer bundles are not source additions.
        if name.startswith('installer/v0.4.1/') or not path.is_file():
            continue
        if path.suffix in {'.key', '.wav', '.pcm', '.bin', '.debug', '.zip'} or path.stat().st_size > 2_000_000:
            raise SystemExit(f'Review unexpected new asset before snapshot: {name}')
        result = subprocess.run(
            ['git', 'diff', '--no-index', '--binary', '--no-ext-diff', '--no-textconv',
             '--', '/dev/null', name], cwd=PULSAR, capture_output=True)
        if result.returncode not in (0, 1):
            raise SystemExit(result.stderr.decode())
        patch.extend(result.stdout)
        additions.append(name)
    (OUT / 'local.patch').write_bytes(patch)
    (OUT / 'base.json').write_text(json.dumps({
        'base_commit': base,
        'base_description': git('describe', '--tags', '--always', 'HEAD').decode().strip(),
        'branch': git('branch', '--show-current').decode().strip(),
        'new_source_files': additions,
    }, ensure_ascii=False, indent=2) + '\n')
    print(f'Pulsar snapshot: {len(patch)} bytes, {len(additions)} new files, base {base[:12]}')

if __name__ == '__main__':
    main()
