#!/usr/bin/env bash
#MISE description="Documented rollback for one unit (prints the plan; --yes executes it)"
#USAGE arg "<unit>" help="Unit to roll back (see `mise run units`)"
#USAGE complete "unit" run="node platform/mise-tasks/lib/units-cli.mjs ids"
#USAGE flag "-y --yes" help="Execute the printed rollback command"
#USAGE flag "--dry-run" help="Print the plan and exit without executing"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec node "$ROOT/platform/mise-tasks/lib/ops.mjs" rollback "$@"
