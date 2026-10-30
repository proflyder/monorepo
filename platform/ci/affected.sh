#!/usr/bin/env bash
# Local reproduction of the CI routing decision. See docs/TESTING.md.
#
#   platform/ci/affected.sh --base origin/develop --format text
#   platform/ci/affected.sh --changed-file solutions/scheduler/internal/store/tick.go
#   platform/ci/affected.sh --base HEAD~5 --format json
#
# The script only reads Git and the Bazel loading graph; it never builds,
# deploys or touches a device.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec python3 "$ROOT/platform/ci/affected.py" --root "$ROOT" "$@"
