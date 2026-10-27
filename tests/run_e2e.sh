#!/usr/bin/env bash
# Opt-in live E2E. Requires HA_TOKEN; gateway checks additionally require
# STATION_CONTROL_TOKEN. Audio mutation is separately opt-in.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
: "${HA_TOKEN:?set HA_TOKEN (do not commit it)}"
export STATION_E2E=1
export HA_URL="${HA_URL:-http://192.0.2.25:8123}"

echo "== HA read-only E2E =="
python3 -m unittest tests.e2e.live_ha -v

if [[ -n "${STATION_CONTROL_TOKEN:-}" ]]; then
  echo "== BLE gateway read-only E2E =="
  python3 -m unittest tests.e2e.live_gateway -v
else
  echo "SKIP gateway E2E: STATION_CONTROL_TOKEN is not set"
fi

if [[ -n "${HERMES_API_KEY:-}" ]]; then
  echo "== Hermes read-only E2E =="
  python3 -m unittest tests.e2e.live_hermes -v
else
  echo "SKIP Hermes E2E: HERMES_API_KEY is not set"
fi
