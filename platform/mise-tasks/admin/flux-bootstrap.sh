#!/usr/bin/env bash
#MISE description="Install Flux (source/kustomize/helm controllers) and hand Nexus reconciliation to Git"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P kube exec -- "$ROOT/platform/workstation/with-kube-tunnel.sh" "$ROOT/projects/home/kubernetes/flux/bootstrap.sh" "$@"
