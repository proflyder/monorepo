#!/usr/bin/env bash
# Read-only acceptance checks for the private home HTTPS suffix.
#
#   HOME_DOMAIN=home.example.com NODE_IP=192.0.2.27 ./verify.sh
#
# Optional bearer tokens (scheduler + HA) are read from private files; they are
# never printed. WebSocket and issuer checks are included.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"

PRIVATE_ENV="${INGRESS_PRIVATE_ENV:-$ROOT/secrets/home/ingress.env}"
if [[ -f "$PRIVATE_ENV" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$PRIVATE_ENV"
  set +a
fi

: "${HOME_DOMAIN:?Set HOME_DOMAIN}"
NODE_IP="${NODE_IP:-192.0.2.27}"
SCHEDULER_TOKEN_FILE="${SCHEDULER_TOKEN_FILE:-$ROOT/secrets/home/scheduler.token}"
HA_TOKEN_FILE="${HA_TOKEN_FILE:-$ROOT/secrets/home/ha.token}"

fail=0
check() {
  local label=$1 expected=$2 actual=$3
  if [[ "$actual" == "$expected" ]]; then
    printf 'OK   %-34s %s\n' "$label" "$actual"
  else
    printf 'FAIL %-34s expected %s, got %s\n' "$label" "$expected" "$actual"
    fail=1
  fi
}

# Accept any of a pipe-separated set, e.g. "200|204".
check_any() {
  local label=$1 expected=$2 actual=$3
  if [[ "|$expected|" == *"|$actual|"* ]]; then
    printf 'OK   %-34s %s\n' "$label" "$actual"
  else
    printf 'FAIL %-34s expected %s, got %s\n' "$label" "$expected" "$actual"
    fail=1
  fi
}

for host in ha grafana alarms control; do
  name="$host.$HOME_DOMAIN"
  resolved=$(dig +short A "$name" @harmony.ns.cloudflare.com | tail -1)
  check "dns $host" "$NODE_IP" "$resolved"
  code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 \
    --resolve "$name:443:$NODE_IP" "https://$name/" || true)
  case "$host" in
    ha) check "https $host /" "200" "$code" ;;
    grafana) check_any "https $host /" "301|302" "$code" ;;
    alarms) check "https $host /" "404" "$code" ;;
    control) check "https $host /" "404" "$code" ;;
  esac
  redirect=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 \
    --resolve "$name:80:$NODE_IP" "http://$name/" || true)
  check_any "redirect $host" "301|308" "$redirect"
done

health=$(curl -s --max-time 10 --resolve "grafana.$HOME_DOMAIN:443:$NODE_IP" \
  "https://grafana.$HOME_DOMAIN/api/health" || true)
if [[ "$health" == *'"database": "ok"'* ]]; then
  check "grafana health" "ok" "ok"
else
  check "grafana health" "ok" "bad"
fi

healthz=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 \
  --resolve "control.$HOME_DOMAIN:443:$NODE_IP" \
  "https://control.$HOME_DOMAIN/healthz" || true)
check_any "control /healthz" "200|204" "$healthz"

if [[ -r "$SCHEDULER_TOKEN_FILE" ]]; then
  token=$(tr -d '\r\n' <"$SCHEDULER_TOKEN_FILE")
  catalog=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 \
    --resolve "control.$HOME_DOMAIN:443:$NODE_IP" \
    -H "Authorization: Bearer $token" \
    "https://control.$HOME_DOMAIN/v1/catalog" || true)
  check "control /v1/catalog" "200" "$catalog"
  status=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 \
    --resolve "alarms.$HOME_DOMAIN:443:$NODE_IP" \
    -H "Authorization: Bearer $token" -H 'Content-Type: application/json' \
    -d '{}' \
    "https://alarms.$HOME_DOMAIN/ricardo.v1.SchedulerService/GetStatus" || true)
  check "scheduler GetStatus" "200" "$status"
else
  echo "SKIP scheduler/control bearer checks (no $SCHEDULER_TOKEN_FILE)"
fi

issuer=$(openssl s_client -connect "$NODE_IP:443" -servername "grafana.$HOME_DOMAIN" \
  </dev/null 2>/dev/null | openssl x509 -noout -issuer 2>/dev/null || true)
if [[ "$issuer" == *"Let's Encrypt"* && "$issuer" != *"STAGING"* ]]; then
  check "certificate issuer" "prod" "prod"
else
  check "certificate issuer" "prod" "$issuer"
fi

ws_key=$(openssl rand -base64 16)
ws=$(curl -s -o /dev/null -w '%{http_code}' --http1.1 --max-time 5 -N \
  --resolve "ha.$HOME_DOMAIN:443:$NODE_IP" \
  -H 'Connection: Upgrade' -H 'Upgrade: websocket' \
  -H 'Sec-WebSocket-Version: 13' \
  -H "Sec-WebSocket-Key: $ws_key" \
  "https://ha.$HOME_DOMAIN/api/websocket" || true)
check "HA websocket /api/websocket" "101" "$ws"

exit "$fail"
