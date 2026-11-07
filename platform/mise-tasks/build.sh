#!/usr/bin/env bash
#MISE description="bazel build //..., or one CI unit's Bazel targets / Go binaries"
#USAGE arg "[unit]" help="Build only this CI unit's targets (see `mise run units`)"
#USAGE complete "unit" run="node platform/mise-tasks/lib/units-cli.mjs ids"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec node "$ROOT/platform/mise-tasks/lib/units-cli.mjs" build ${usage_unit:+"$usage_unit"}
