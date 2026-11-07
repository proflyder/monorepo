#!/usr/bin/env bash
#MISE description="Hermetic test suite (Python, Go, Node, deploy control); optional single CI unit"
#USAGE arg "[unit]" help="Only run this CI unit's tests (see `mise run units`)"
#USAGE complete "unit" run="node platform/mise-tasks/lib/units-cli.mjs ids"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec node "$ROOT/platform/mise-tasks/lib/units-cli.mjs" test ${usage_unit:+"$usage_unit"}
