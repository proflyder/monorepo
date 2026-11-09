#!/usr/bin/env bash
# Apply the ExternalSecret manifests that belong to `deploy-home` services.
#
# The External Secrets Operator itself, ClusterSecretStore/openbao and the
# infrastructure ExternalSecrets (cert-manager, monitoring, vaultwarden) are
# managed by Flux: `projects/home/kubernetes/flux/cluster/external-secrets.yaml`
# and the component Kustomizations. This script only covers the application
# secrets whose workloads remain on `deploy-home` (scheduler, Hermes,
# currency-bot, mcp-gateway, wakegrant), so an operator/CRD change never needs
# this manual step.
#
# It never writes secret values: values live only in OpenBao
# (`secret/k8s/<namespace>/<secret-name>`). See README.md for the migration and
# rotation procedure.
#
# Usage:
#   export KUBECONFIG=/private/path/kubeconfig
#   ./projects/home/kubernetes/external-secrets/deploy.sh
#   ./projects/home/kubernetes/external-secrets/deploy.sh --skip-secrets
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck source=platform/deploy/resolve.sh
source "$ROOT/platform/deploy/resolve.sh"

: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"
command -v kubectl >/dev/null || { echo "kubectl is required" >&2; exit 1; }

APPLY_SECRETS=1
for arg in "$@"; do
  case "$arg" in
    --skip-secrets) APPLY_SECRETS=0 ;;
    -h|--help) sed -n '2,20p' "$0"; exit 0 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

kubectl get crd externalsecrets.external-secrets.io >/dev/null
kubectl -n external-secrets rollout status deployment/external-secrets --timeout=180s
kubectl wait --for=condition=Ready clustersecretstore/openbao --timeout=120s

if [[ "$APPLY_SECRETS" == 0 ]]; then
  echo "External Secrets Operator is Ready; skipped application ExternalSecrets"
  exit 0
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
chmod 700 "$TMP"

# ExternalSecret objects for applications still deployed by `deploy-home`.
# Infrastructure ExternalSecrets are applied by their Flux Kustomization.
manifests=(
  "$ROOT/projects/home/kubernetes/scheduler/manifests/external-secrets.yaml"
  "$ROOT/projects/home-assistant/deploy/manifests/external-secrets.yaml"
  "$ROOT/projects/home/kubernetes/currency-bot/manifests/external-secrets.yaml"
  "$ROOT/projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml"
  "$ROOT/projects/home/kubernetes/wakegrant/manifests/external-secrets.yaml"
)
for manifest in "${manifests[@]}"; do
  kubectl apply -f "$manifest"
done

# `kubectl wait` needs explicit names, so resolve them from the applied files.
home_deploy eso names --out "$TMP/names" "${manifests[@]}" >/dev/null
while IFS= read -r item; do
  [[ -n "$item" ]] || continue
  ns=${item%%/*}
  name=${item##*/}
  kubectl -n "$ns" wait --for=condition=Ready "externalsecret/$name" --timeout=120s
done < "$TMP/names"

echo "Application ExternalSecrets are Ready; ESO itself is managed by Flux"
