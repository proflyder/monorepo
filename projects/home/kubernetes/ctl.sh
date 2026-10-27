#!/usr/bin/env bash
# Host operations only. Helm releases remain the deployment source of truth.
set -euo pipefail
KUBE=(sudo -n k3s kubectl)
service_name() {
  case "${1:-}" in
    ha) echo homeassistant ;;
    ma|music) echo music-assistant ;;
    matter-server) echo matter ;;
    onnx-stt) echo gigaam ;;
    *) printf '%s\n' "${1:-}" ;;
  esac
}
case "${1:-status}" in
  status)
    "${KUBE[@]}" get nodes
    "${KUBE[@]}" get pods -A
    df -h / /srv/k3s
    ;;
  logs)
    name="$(service_name "${2:?Specify a deployment}")"
    extra=()
    [[ -z "${3:-}" ]] || extra=(-c "$3")
    "${KUBE[@]}" -n home-assistant logs "deploy/$name" "${extra[@]}" --tail=100 -f
    ;;
  restart)
    name="$(service_name "${2:?Specify one deployment}")"
    "${KUBE[@]}" -n home-assistant rollout restart "deploy/$name"
    "${KUBE[@]}" -n home-assistant rollout status "deploy/$name" --timeout=5m
    ;;
  doctor)
    "${KUBE[@]}" get events -A --field-selector type=Warning --sort-by=.lastTimestamp
    "${KUBE[@]}" top nodes
    "${KUBE[@]}" get pvc -A
    ;;
  help|--help|-h)
    echo 'Usage: ctl.sh status | logs SERVICE [CONTAINER] | restart SERVICE | doctor'
    echo 'Deploy/update with pinned Helm charts; backup/restore: docs/KUBERNETES-OPERATIONS.md'
    ;;
  *) echo 'Unsupported Kubernetes operation; use --help. Docker commands have been retired.' >&2; exit 2 ;;
esac
