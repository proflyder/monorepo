#!/usr/bin/env bash
#MISE description="Merge a PR into develop only when ci-ok ran on the tree that lands (refreshes a stale branch)"
#USAGE arg "[pr]" help="Pull request number"
#USAGE flag "--dry-run" help="Check everything and print what would run; update/merge nothing"
#USAGE flag "--no-update" help="Only verify: refuse a head that lacks develop; never update or merge"
#USAGE flag "--timeout <min>" default="90" help="How long to wait for ci-ok on the current head"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec node "$ROOT/platform/ci/safe-merge.mjs" "$@"
