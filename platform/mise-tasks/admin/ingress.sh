#!/usr/bin/env bash
#MISE description="Deploy Traefik + cert-manager and the shared routes (applies; --staging, --bootstrap-secrets, --help)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P ingress exec -- "$ROOT/platform/workstation/with-kube-tunnel.sh" "$ROOT/projects/home/kubernetes/ingress/deploy.sh" "$@"
