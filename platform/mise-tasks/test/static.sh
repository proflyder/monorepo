#!/usr/bin/env bash
#MISE description="Static gate over first-party Python product scripts"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
exec python3 -m unittest discover -s platform/quality -p 'test_static.py' -v
