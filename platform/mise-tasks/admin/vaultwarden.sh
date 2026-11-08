#!/usr/bin/env bash
#MISE description="Deploy Vaultwarden (--render-only DIR, --skip-dns)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P vaultwarden exec -- "$ROOT/platform/workstation/with-kube-tunnel.sh" "$ROOT/projects/home/kubernetes/vaultwarden/deploy.sh" "$@"
