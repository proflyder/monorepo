#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=platform/deploy/resolve.sh
source "$ROOT/platform/deploy/resolve.sh"
: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"
: "${RUNTIME_PRIVATE_VALUES:?Set RUNTIME_PRIVATE_VALUES to the private runtime values}"
: "${HEALTH_EXPORTER_IMAGE:?Set HEALTH_EXPORTER_IMAGE to the digest-pinned ghcr.io/proflyder/health-exporter image}"
KUBECTL=${KUBECTL:-kubectl}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
chmod 700 "$TMP"

helm template runtime "$ROOT/projects/home-assistant/deploy/chart" \
  -n home-assistant -f "$RUNTIME_PRIVATE_VALUES" > "$TMP/render.yaml"

home_deploy runtime split \
  --render "$TMP/render.yaml" \
  --private "$RUNTIME_PRIVATE_VALUES" \
  --storage-out "$TMP/storage.yaml" \
  --values-out "$TMP/values.yaml" \
  --health-exporter-image "$HEALTH_EXPORTER_IMAGE" >/dev/null
chmod 600 "$TMP/storage.yaml" "$TMP/values.yaml"

if [[ -s "$TMP/storage.yaml" ]]; then
  "$KUBECTL" -n home-assistant apply -f "$TMP/storage.yaml"
fi

"$KUBECTL" apply -f "$ROOT/projects/home-assistant/deploy/manifests/external-secrets.yaml"
"$KUBECTL" -n home-assistant wait --for=condition=Ready externalsecret/hermes-environment --timeout=120s

helm upgrade --install runtime "$ROOT/projects/home-assistant/deploy/chart" \
  -n home-assistant -f "$TMP/values.yaml" --wait --timeout 8m
