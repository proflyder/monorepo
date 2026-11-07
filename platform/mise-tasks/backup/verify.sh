#!/usr/bin/env bash
#MISE description="Run documented backup verification (restic check, integrity checks; --dry-run prints)"
#USAGE arg "[targets]" var=#true help="Backup targets (comma list) or all; default all"
#USAGE complete "targets" run="node platform/mise-tasks/lib/backup.mjs ids"
#USAGE flag "-y --yes" help="Skip the confirmation prompt (non-interactive)"
#USAGE flag "--dry-run" help="Print the verification commands and exit without running them"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec node "$ROOT/platform/mise-tasks/lib/backup.mjs" verify "$@"
