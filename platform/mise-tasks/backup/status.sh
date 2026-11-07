#!/usr/bin/env bash
#MISE description="Read-only backup freshness/status probe (ssh/kubectl; --dry-run prints commands)"
#USAGE arg "[targets]" var=#true help="Backup targets (comma list) or all; default all"
#USAGE complete "targets" run="node platform/mise-tasks/lib/backup.mjs ids"
#USAGE flag "--dry-run" help="Print the documented commands and exit without probing"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec node "$ROOT/platform/mise-tasks/lib/backup.mjs" status "$@"
