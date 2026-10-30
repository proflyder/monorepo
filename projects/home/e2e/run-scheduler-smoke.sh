#!/usr/bin/env bash
# Local-only scheduler + ha-bridge E2E. Builds binaries into a temp dir and
# uses loopback fake HA; never touches live HA/ADB.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec python3 "$ROOT/projects/home/e2e/scheduler_smoke.py" "$@"
