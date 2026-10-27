#!/usr/bin/env bash
# Waits until the station fetches /get_sync_info through the proxy, then
# connects ADB over the network.
#
#   ./scripts/wait-for-adb.sh
#
# Prerequisite: the station was rebooted (full sync happens on boot).
set -euo pipefail

ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
LOG="${LOG_FILE:-${ROOT}/logs/requests.jsonl}"
env_station="$(sed -n 's/^STATION_IP=//p' "${ROOT}/.env" 2>/dev/null | tail -1)"
STATION_IP="${STATION_IP:-${env_station:-192.0.2.13}}"
ADB="${ADB:-${HOME}/Library/Android/sdk/platform-tools/adb}"

echo "waiting for ${STATION_IP} in ${LOG} ..."
start_size="$(wc -c < "${LOG}" 2>/dev/null || echo 0)"
while ! tail -c +"$((start_size + 1))" "${LOG}" 2>/dev/null | grep -q '"action":"sync"'; do
  sleep 2
done

echo "sync config served, waiting for adbd on ${STATION_IP}:5555 ..."
for _ in $(seq 1 60); do
  if nc -z -G 1 "${STATION_IP}" 5555 2>/dev/null; then
    break
  fi
  sleep 2
done

"${ADB}" connect "${STATION_IP}:5555"
"${ADB}" wait-for-device
"${ADB}" shell getprop ro.product.model
"${ADB}" shell id

echo
echo "Next: ./scripts/install-ca.sh ${ROOT}/certs/ca.crt"
