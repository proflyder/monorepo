#!/usr/bin/env bash
#MISE description="Deploy home units via deploy-home.yml (gh workflow run; --dry-run prints the command)"
#USAGE arg "[units]" var=#true help="Deployable units, a comma list, or all (see `mise run units`)"
#USAGE complete "units" run="node platform/mise-tasks/lib/units-cli.mjs ids"
#USAGE flag "-y --yes" help="Skip the confirmation prompt (non-interactive)"
#USAGE flag "--dry-run" help="Print the gh command and exit without dispatching"
#USAGE flag "--watch" help="Watch the dispatched run to completion"
#USAGE flag "--mode <mode>" default="preserve" help="Deploy mode: preserve, execute, shadow, control-execute, control-catalog"
#USAGE flag "--simulate-hermes-failure" help="Rollback drill: fail the Hermes smoke after the upgrade (hermes unit only)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec node "$ROOT/platform/mise-tasks/lib/ops.mjs" deploy "$@"
