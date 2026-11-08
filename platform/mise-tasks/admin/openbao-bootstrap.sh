#!/usr/bin/env bash
#MISE description="OpenBao bootstrap/configure (idempotent; asks for a human admin token)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P openbao exec -- "$ROOT/platform/workstation/with-kube-tunnel.sh" "$ROOT/projects/home/kubernetes/openbao/bootstrap.sh" "$@"
