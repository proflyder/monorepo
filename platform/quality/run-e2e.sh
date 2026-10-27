#!/usr/bin/env bash
# Repository convenience command; each project owns its live tests.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
"$ROOT/projects/home-assistant/scripts/test-e2e.sh"
if [[ -n "${STATION_CONTROL_TOKEN:-}" ]]; then
  "$ROOT/projects/station/scripts/test-e2e.sh"
else
  echo "SKIP station E2E: STATION_CONTROL_TOKEN is not set"
fi
