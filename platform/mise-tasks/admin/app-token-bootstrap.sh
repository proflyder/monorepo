#!/usr/bin/env bash
#MISE description="Bootstrap/rotate app + service tokens in OpenBao, force ESO and restart consumers (prints owner token once)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUN="$ROOT/platform/workstation/run.sh"
PY="$ROOT/platform/workstation/app_tokens.py"

force_sync() {
  mise run kubectl -- -n home-assistant annotate externalsecret scheduler-auth \
    force-sync="$(date +%s)" --overwrite >/dev/null
  mise run kubectl -- -n home-assistant wait --for=condition=Ready \
    externalsecret/scheduler-auth --timeout=180s
}

restart() {
  local name="$1"
  if mise run kubectl -- -n home-assistant get deployment "$name" >/dev/null 2>&1; then
    mise run kubectl -- -n home-assistant rollout restart "deployment/$name" >/dev/null
    mise run kubectl -- -n home-assistant rollout status "deployment/$name" --timeout=180s
  fi
}

if [[ " $* " == *" --finalize "* ]]; then
  "$RUN" "$PY" "$@"
  force_sync
  restart scheduler
  exit 0
fi

"$RUN" "$PY" "$@"
if [[ " $* " == *" --dry-run "* ]]; then
  exit 0
fi

force_sync
restart scheduler
for name in phone-push mcp-gateway intercom ha-control ha-bridge ha-scenario hermes voice-proxy; do
  restart "$name"
done
"$RUN" "$PY" --finalize
force_sync
restart scheduler
printf '%s\n' "Legacy token is no longer a service token; it remains the app token «Мама»."