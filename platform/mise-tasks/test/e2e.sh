#!/usr/bin/env bash
#MISE description="Live E2E against HA/BLE, read-only (needs HA_TOKEN etc.)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/quality/run-e2e.sh"
