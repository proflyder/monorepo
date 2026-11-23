#!/usr/bin/env bash
#MISE description="CI check of one Bazel unit: one keep-going bazel call over its tests and build targets"
#USAGE arg "<unit>" help="Bazel or Android CI unit (see `mise run units`)"
#USAGE complete "unit" run="node platform/mise-tasks/lib/units-cli.mjs ids"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec node "$ROOT/platform/mise-tasks/lib/units-cli.mjs" ci "$usage_unit"
