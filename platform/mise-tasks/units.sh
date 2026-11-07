#!/usr/bin/env bash
#MISE description="List CI units: id, platform, title, deployable"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec node "$ROOT/platform/mise-tasks/lib/units-cli.mjs" units
