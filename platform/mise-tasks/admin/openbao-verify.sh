#!/usr/bin/env bash
#MISE description="Verify OpenBao HTTPS/UI/policies (read-only; --kubernetes adds a pod probe)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P openbao exec -- "$ROOT/platform/workstation/with-kube-tunnel.sh" "$ROOT/projects/home/kubernetes/openbao/verify.sh" "$@"
