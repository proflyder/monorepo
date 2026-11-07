#!/usr/bin/env bash
#MISE description="Run one documented backup now (plan + --yes; --dry-run prints the command)"
#USAGE arg "<target>" help="Backup target (see `mise run backup:list`)"
#USAGE complete "target" run="node platform/mise-tasks/lib/backup.mjs ids"
#USAGE flag "-y --yes" help="Skip the confirmation prompt (non-interactive)"
#USAGE flag "--dry-run" help="Print the plan and exit without running the backup"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec node "$ROOT/platform/mise-tasks/lib/backup.mjs" now "$@"
