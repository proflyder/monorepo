#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$ROOT/projects/home/kubernetes/versions.env"
: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"
# Bootstrap namespaces, storage and grafana-admin Secret before running this.
# Real server URL overrides belong in a private values file, never in Git.
extra=()
if [[ -n "${PROMETHEUS_PRIVATE_VALUES:-}" ]]; then
  extra=(-f "$PROMETHEUS_PRIVATE_VALUES")
fi
helm upgrade --install monitoring prometheus-community/kube-prometheus-stack \
  --version "$PROMETHEUS_CHART_VERSION" -n monitoring \
  -f "$ROOT/platform/observability/values/prometheus.yaml" "${extra[@]}" --wait --timeout 8m
helm upgrade --install loki grafana-community/loki --version "$LOKI_CHART_VERSION" \
  -n monitoring -f "$ROOT/platform/observability/values/loki.yaml" --wait --timeout 8m
helm upgrade --install alloy grafana/alloy --version "$ALLOY_CHART_VERSION" \
  -n monitoring -f "$ROOT/platform/observability/values/alloy.yaml" --wait --timeout 5m
helm upgrade --install ha-observability "$ROOT/projects/home-assistant/observability/chart" \
  -n monitoring --wait --timeout 2m
