#!/usr/bin/env bash
# Apply the full runtime state from the private values file.
#
# The chart is split into two ownership domains on purpose:
#   * PersistentVolume/PersistentVolumeClaim ("storage") are applied by
#     kubectl. They are cluster-scoped/retained data, and CI must never need
#     cluster-scoped RBAC or be able to delete a volume.
#   * everything else is a regular Helm release ("workloads"), which the
#     deploy-home workflow may roll by digest.
# Both halves come from the same private values file, so a manual apply and the
# CI rollout render identical objects. Run this script as a cluster admin; the
# CI ServiceAccount only has the workload half.
#
# `HEALTH_EXPORTER_IMAGE` must be the digest-pinned
# `ghcr.io/proflyder/health-exporter@sha256:…` image. The chart no longer ships
# the Python exporter script/ConfigMap; the image, command and volume mounts
# are pinned here (and by `home-deploy hermes rollout` in CI), so the private
# values do not have to change. Read the current digest with:
#   kubectl -n home-assistant get deployment runtime-health-exporter \
#     -o jsonpath='{.spec.template.spec.containers[0].image}'
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
# shellcheck source=platform/deploy/resolve.sh
source "$ROOT/platform/deploy/resolve.sh"
: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"
: "${RUNTIME_PRIVATE_VALUES:?Set RUNTIME_PRIVATE_VALUES to the private runtime values}"
: "${HEALTH_EXPORTER_IMAGE:?Set HEALTH_EXPORTER_IMAGE to the digest-pinned ghcr.io/proflyder/health-exporter image}"
KUBECTL=${KUBECTL:-kubectl}

# Existing data directories and local images must exist. For initial migration,
# stop each previous writer before increasing that workload's replica count.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
chmod 700 "$TMP"

helm template runtime "$ROOT/projects/home-assistant/deploy/chart" \
  -n home-assistant -f "$RUNTIME_PRIVATE_VALUES" > "$TMP/render.yaml"

# Split the render: keep PV/PVC out of the Helm release and write a values file
# without `storage`, with the health exporter pinned to the Rust image.
home_deploy runtime split \
  --render "$TMP/render.yaml" \
  --private "$RUNTIME_PRIVATE_VALUES" \
  --storage-out "$TMP/storage.yaml" \
  --values-out "$TMP/values.yaml" \
  --health-exporter-image "$HEALTH_EXPORTER_IMAGE" >/dev/null
chmod 600 "$TMP/storage.yaml" "$TMP/values.yaml"

# Apply storage first: this also stamps the keep annotation on volumes that an
# older release still tracks, so the Helm upgrade below can drop them from the
# release without deleting them.
if [[ -s "$TMP/storage.yaml" ]]; then
  "$KUBECTL" -n home-assistant apply -f "$TMP/storage.yaml"
fi

# hermes-environment is owned by External Secrets Operator (OpenBao
# secret/k8s/home-assistant/hermes-environment). Applying the manifest here
# keeps a manual full apply self-consistent; the values still come only from
# OpenBao.
"$KUBECTL" apply -f "$ROOT/projects/home-assistant/deploy/manifests/external-secrets.yaml"
"$KUBECTL" -n home-assistant wait --for=condition=Ready externalsecret/hermes-environment --timeout=120s

helm upgrade --install runtime "$ROOT/projects/home-assistant/deploy/chart" \
  -n home-assistant -f "$TMP/values.yaml" --wait --timeout 8m
