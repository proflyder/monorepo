#!/usr/bin/env bash
#MISE description="Deploy ESO + ClusterSecretStore and apply ExternalSecrets (applies; --skip-secrets, --help)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P kube exec -- "$ROOT/platform/workstation/with-kube-tunnel.sh" "$ROOT/projects/home/kubernetes/external-secrets/deploy.sh" "$@"
