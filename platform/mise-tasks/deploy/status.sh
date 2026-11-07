#!/usr/bin/env bash
#MISE description="Last deploy-home runs, and how to check one unit in the cluster (--live runs read-only kubectl get deploy)"
#USAGE arg "[unit]" help="Only show this unit's namespace/workloads and check command (see `mise run units`)"
#USAGE complete "unit" run="node platform/mise-tasks/lib/units-cli.mjs ids"
#USAGE flag "--live" help="Run read-only `kubectl get deploy` for the unit (needs the admin kubeconfig)"
#USAGE flag "--limit <limit>" default="5" help="How many deploy-home runs to list"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec node "$ROOT/platform/mise-tasks/lib/ops.mjs" deploy-status "$@"
