#!/usr/bin/env bash
#MISE description="Bootstrap/rotate app + service tokens in OpenBao, force ESO and restart consumers (prints owner token once)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUN="$ROOT/platform/workstation/run.sh"
PY="$ROOT/platform/workstation/app_tokens.py"

kube() {
  "$ROOT/platform/workstation/with-kube-tunnel.sh" kubectl "$@"
}

force_sync() {
  for name in scheduler-auth phone-push mcp-gateway-scheduler intercom; do
    kube -n home-assistant annotate "externalsecret/$name" \
      force-sync="$(date +%s)" --overwrite >/dev/null
  done
  for name in scheduler-auth phone-push mcp-gateway-scheduler intercom; do
    kube -n home-assistant wait --for=condition=Ready \
      "externalsecret/$name" --timeout=180s
  done
}

verify() {
  "$ROOT/platform/workstation/fnox.sh" -P kube exec -- "$RUN" "$PY" --verify-service-token
}

restart() {
  local name="$1"
  kube -n home-assistant get deployment "$name" >/dev/null
  kube -n home-assistant rollout restart "deployment/$name" >/dev/null
  kube -n home-assistant rollout status "deployment/$name" --timeout=180s
}

if [[ " $* " == *" --finalize "* ]]; then
  "$ROOT/platform/workstation/fnox.sh" -P kube exec -- "$RUN" "$PY" "$@"
  force_sync
  restart scheduler
  verify
  exit 0
fi

"$ROOT/platform/workstation/fnox.sh" -P kube exec -- "$RUN" "$PY" "$@"
if [[ " $* " == *" --dry-run "* ]]; then
  exit 0
fi

force_sync
restart scheduler
for name in phone-push mcp-gateway intercom ha-control ha-bridge ha-scenario hermes voice-proxy; do
  restart "$name"
done
verify
"$ROOT/platform/workstation/fnox.sh" -P kube exec -- "$RUN" "$PY" --finalize
force_sync
restart scheduler
verify
printf '%s\n' "Legacy token is no longer a service token; it remains the app token «Мама»."