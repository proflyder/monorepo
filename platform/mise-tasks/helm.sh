#!/usr/bin/env bash
#MISE description="helm against home K3s: admin kubeconfig from OpenBao (fnox, profile kube), API tunnel for the command"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P kube exec -- "$ROOT/platform/workstation/with-kube-tunnel.sh" helm "$@"
