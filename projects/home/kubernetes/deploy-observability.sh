#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
source "$ROOT/projects/home/kubernetes/versions.sh"
: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"
# grafana-admin is owned by External Secrets Operator (OpenBao
# secret/k8s/monitoring/grafana-admin). Requires ESO and OpenBao; see
# projects/home/kubernetes/external-secrets/README.md.
kubectl apply -f "$ROOT/projects/home/kubernetes/observability/manifests/external-secret-grafana-admin.yaml"
kubectl -n monitoring wait --for=condition=Ready externalsecret/grafana-admin --timeout=120s
# alertmanager-telegram renders alertmanager.yaml (Telegram receiver) from
# k8s/home-assistant/hermes-environment (TELEGRAM_BOT_TOKEN /
# TELEGRAM_ALLOWED_USERS, the dedicated infra bot); Alertmanager reads it via
# configSecret. The always-firing Watchdog is routed to the external heartbeat
# Worker with url/token from k8s/monitoring/heartbeat.
kubectl apply -f "$ROOT/projects/home/kubernetes/observability/manifests/external-secret-alertmanager-telegram.yaml"
kubectl -n monitoring wait --for=condition=Ready externalsecret/alertmanager-telegram --timeout=120s
kubectl -n monitoring create configmap alertmanager-templates \
  --from-file="$ROOT/projects/home/kubernetes/observability/alertmanager/telegram.tmpl" \
  --dry-run=client -o yaml | kubectl apply -f -
# Real server URLs and scrape targets belong in a private values file, never in
# Git (example: projects/home/kubernetes/observability/pve-scrape.values.example.yaml).
private_values="${PROMETHEUS_PRIVATE_VALUES:-$ROOT/secrets/home/observability-values.yaml}"
extra=()
if [[ -f "$private_values" ]]; then
  extra=(-f "$private_values")
fi
helm upgrade --install monitoring prometheus-community/kube-prometheus-stack \
  --version "$PROMETHEUS_CHART_VERSION" -n monitoring \
  -f "$ROOT/platform/observability/values/prometheus.yaml" "${extra[@]}" --wait --timeout 8m
# Backup freshness alerts (PVE vzdump + K3s offsite). ruleSelector is open, so
# no Helm release label is required.
kubectl apply -f "$ROOT/projects/home/kubernetes/observability/manifests/prometheusrule-backups.yaml"
helm upgrade --install loki grafana-community/loki --version "$LOKI_CHART_VERSION" \
  -n monitoring -f "$ROOT/platform/observability/values/loki.yaml" --wait --timeout 8m
helm upgrade --install alloy grafana/alloy --version "$ALLOY_CHART_VERSION" \
  -n monitoring -f "$ROOT/platform/observability/values/alloy.yaml" --wait --timeout 5m
helm upgrade --install ha-observability "$ROOT/projects/home-assistant/observability/chart" \
  -n monitoring --wait --timeout 2m
