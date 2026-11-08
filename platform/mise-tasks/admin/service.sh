#!/usr/bin/env bash
#MISE description="Run a service deploy.sh by hand with its OpenBao secrets (scheduler, currency-bot, mcp-gateway, wakegrant); CI normally does this"
#USAGE arg "<unit>" help="scheduler, currency-bot, mcp-gateway or wakegrant"
#USAGE arg "[args]" var=#true help="Arguments for the unit's deploy.sh"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
unit="${usage_unit:?unit}"
case "$unit" in
  scheduler|currency-bot|mcp-gateway|wakegrant) ;;
  *) echo "admin:service: unknown unit '$unit' (scheduler, currency-bot, mcp-gateway, wakegrant)" >&2; exit 2 ;;
esac
# shellcheck disable=SC2086 # usage_args is mise's space-joined var=true list
exec "$ROOT/platform/workstation/fnox.sh" -P "$unit" exec -- \
  "$ROOT/platform/workstation/with-kube-tunnel.sh" "$ROOT/projects/home/kubernetes/$unit/deploy.sh" ${usage_args:-}
