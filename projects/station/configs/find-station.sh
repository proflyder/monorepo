#!/usr/bin/env bash
# Resolve the station's current LAN address via mDNS (the hostname embeds the
# device id and survives DHCP changes and factory resets).
#
#   ./scripts/find-station.sh          # print ip + adb state
#   ./scripts/find-station.sh --connect
set -euo pipefail

LAB_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
if [[ -z "${STATION_HOST:-}" && -r "$LAB_ROOT/secrets/station-host.txt" ]]; then
  STATION_HOST="$(cat "$LAB_ROOT/secrets/station-host.txt")"
fi
: "${STATION_HOST:?Set STATION_HOST to your station mDNS hostname}"
ADB="${ADB:-adb}"

resolve() {
  local out ip pid
  dns-sd -G v4 "${STATION_HOST}" > /tmp/find-station.out 2>&1 &
  pid=$!
  sleep 3
  kill "${pid}" 2>/dev/null || true
  wait "${pid}" 2>/dev/null || true
  out="$(cat /tmp/find-station.out 2>/dev/null || true)"
  ip="$(printf '%s\n' "${out}" | awk '
    /Add/ {
      for (i = 1; i <= NF; i++) {
        if ($i ~ /^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$/) { print $i; exit }
      }
    }
  ')"
  [[ -n "${ip}" ]] || return 1
  printf '%s' "${ip}"
}

ip="$(resolve || true)"
if [[ -z "${ip}" ]]; then
  echo "station not found via mDNS (${STATION_HOST})" >&2
  exit 1
fi

echo "${ip}"
if nc -z -G 1 "${ip}" 5555 2>/dev/null; then
  echo "adb: open (${ip}:5555)"
  if [[ "${1:-}" == "--connect" ]]; then
    "${ADB}" connect "${ip}:5555"
    "${ADB}" devices
  fi
else
  echo "adb: closed"
fi
