#!/usr/bin/env bash
#MISE description="List every backup target (offline: schedule, where, restore doc)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec node "$ROOT/platform/mise-tasks/lib/backup.mjs" list "$@"
