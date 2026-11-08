#!/usr/bin/env bash
#MISE description="Deploy Prometheus/Loki/Alloy Helm releases (applies)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P observability exec -- "$ROOT/platform/workstation/with-kube-tunnel.sh" "$ROOT/projects/home/kubernetes/deploy-observability.sh" "$@"
