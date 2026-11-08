#!/usr/bin/env bash
#MISE description="Deploy OpenBao (applies; keeps the live seal; --skip-dns, --help)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P openbao exec -- "$ROOT/platform/workstation/with-kube-tunnel.sh" "$ROOT/projects/home/kubernetes/openbao/deploy.sh" "$@"
