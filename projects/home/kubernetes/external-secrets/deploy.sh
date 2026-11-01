#!/usr/bin/env bash
# Deploy External Secrets Operator (ESO) and wire it to OpenBao.
#
# What this script owns:
#   * namespace `external-secrets`;
#   * the pinned `external-secrets` Helm release (chart + digest-pinned image);
#   * ClusterSecretStore `openbao` (Kubernetes auth, KV v2 `secret/`);
#   * applying the ExternalSecret manifests that live next to their projects
#     and waiting for them to report Ready.
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
# shellcheck disable=SC1091
source "$ROOT/projects/home/kubernetes/versions.sh"

: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"
command -v helm >/dev/null || { echo "helm is required" >&2; exit 1; }
command -v kubectl >/dev/null || { echo "kubectl is required" >&2; exit 1; }

APPLY_SECRETS=1
for arg in "$@"; do
  case "$arg" in
    --skip-secrets) APPLY_SECRETS=0 ;;
    -h|--help) sed -n '2,22p' "$0"; exit 0 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

NS=external-secrets
DIR="$ROOT/projects/home/kubernetes/external-secrets"
IMAGE_TAG="${EXTERNAL_SECRETS_IMAGE_VERSION}@${EXTERNAL_SECRETS_IMAGE_DIGEST}"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
chmod 700 "$TMP"

kubectl create namespace "$NS" --dry-run=client -o yaml | kubectl apply -f -

sed "s|__ESO_IMAGE_TAG__|$IMAGE_TAG|g" "$DIR/values/external-secrets.yaml" > "$TMP/values.yaml"

helm repo add external-secrets https://charts.external-secrets.io --force-update >/dev/null
helm repo update external-secrets >/dev/null
helm upgrade --install external-secrets external-secrets/external-secrets \
  --version "$EXTERNAL_SECRETS_CHART_VERSION" -n "$NS" \
  -f "$TMP/values.yaml" --wait --timeout 8m

kubectl -n "$NS" rollout status deployment/external-secrets --timeout=180s
kubectl -n "$NS" rollout status deployment/external-secrets-webhook --timeout=180s
kubectl -n "$NS" rollout status deployment/external-secrets-cert-controller --timeout=180s

kubectl apply -f "$DIR/manifests/clustersecretstore.yaml"
kubectl wait --for=condition=Ready clustersecretstore/openbao --timeout=120s

if [[ "$APPLY_SECRETS" == 1 ]]; then
  # ExternalSecret objects are tracked next to the project they belong to.
  # Applying them here gives one deterministic place to run after OpenBao is
  # configured.
  manifests=(
    "$ROOT/projects/home/kubernetes/scheduler/manifests/external-secrets.yaml"
    "$ROOT/projects/home-assistant/deploy/manifests/external-secrets.yaml"
    "$ROOT/projects/home/kubernetes/ingress/manifests/external-secret-cloudflare.yaml"
    "$ROOT/projects/home/kubernetes/observability/manifests/external-secret-grafana-admin.yaml"
    "$ROOT/projects/home/kubernetes/currency-bot/manifests/external-secrets.yaml"
    "$ROOT/projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml"
  )
  for manifest in "${manifests[@]}"; do
    kubectl apply -f "$manifest"
  done

  # Wait for every ExternalSecret created above. `kubectl wait` needs explicit
  # names, so resolve them from the applied manifests.
  python3 - "$TMP/names" "${manifests[@]}" <<'PY'
import pathlib
import sys

import yaml

out = pathlib.Path(sys.argv[1])
lines = []
for path in sys.argv[2:]:
    for doc in yaml.safe_load_all(pathlib.Path(path).read_text()):
        if doc and doc.get("kind") == "ExternalSecret":
            lines.append(f'{doc["metadata"]["namespace"]}/{doc["metadata"]["name"]}')
out.write_text("\n".join(lines) + "\n")
PY
  while IFS= read -r item; do
    [[ -n "$item" ]] || continue
    ns=${item%%/*}
    name=${item##*/}
    kubectl -n "$ns" wait --for=condition=Ready "externalsecret/$name" --timeout=120s
  done < "$TMP/names"
fi

echo "External Secrets Operator is deployed; ClusterSecretStore openbao is Ready"
