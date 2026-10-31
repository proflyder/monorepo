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
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"
: "${RUNTIME_PRIVATE_VALUES:?Set RUNTIME_PRIVATE_VALUES to the private runtime values}"
KUBECTL=${KUBECTL:-kubectl}

# Existing data directories and local images must exist. For initial migration,
# stop each previous writer before increasing that workload's replica count.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
chmod 700 "$TMP"

helm template runtime "$ROOT/projects/home-assistant/deploy/chart" \
  -n home-assistant -f "$RUNTIME_PRIVATE_VALUES" > "$TMP/render.yaml"

# Split the render: keep PV/PVC out of the Helm release and write a values file
# without `storage`. PyYAML is a test/CI dependency already.
python3 - "$TMP/render.yaml" "$TMP/storage.yaml" "$TMP/values.yaml" "$RUNTIME_PRIVATE_VALUES" <<'PY'
import pathlib
import sys

import yaml

render_path, storage_path, values_path, private_values = map(pathlib.Path, sys.argv[1:])
docs = [doc for doc in yaml.safe_load_all(render_path.read_text()) if doc]
storage = [doc for doc in docs if doc.get("kind") in ("PersistentVolume", "PersistentVolumeClaim")]
storage_path.write_text(yaml.safe_dump_all(storage, default_flow_style=False, sort_keys=False))
values = yaml.safe_load(private_values.read_text()) or {}
values.pop("storage", None)
values_path.write_text(yaml.safe_dump(values, default_flow_style=False, sort_keys=False))
PY
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
