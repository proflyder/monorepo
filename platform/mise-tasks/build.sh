#!/usr/bin/env bash
#MISE description="bazel build //... or the Bazel targets of one CI unit"
#USAGE arg "[unit]" help="Build only this CI unit's Bazel targets (see `mise run units`)"
#USAGE complete "unit" run="node platform/mise-tasks/lib/units-cli.mjs ids"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec node "$ROOT/platform/mise-tasks/lib/units-cli.mjs" build ${usage_unit:+"$usage_unit"}
