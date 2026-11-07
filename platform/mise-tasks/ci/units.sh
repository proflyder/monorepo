#!/usr/bin/env bash
#MISE description="Validate platform/ci/units.mjs and prove every tracked source/test file is routed"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec node "$ROOT/platform/ci/check_units.mjs"
