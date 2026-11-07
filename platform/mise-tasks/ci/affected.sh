#!/usr/bin/env bash
#MISE description="Which CI units and workflows a change range selects"
#USAGE flag "--base <base>" default="origin/develop" help="Base revision of the range"
#USAGE flag "--head <head>" default="HEAD" help="Head revision of the range"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/ci/affected.sh" --base "$usage_base" --head "$usage_head" --format text
