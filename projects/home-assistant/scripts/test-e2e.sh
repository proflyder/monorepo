#!/usr/bin/env bash
# Explicit opt-in live tests; credentials come only from the environment.
set -euo pipefail
PROJECT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
: "${HA_TOKEN:?set HA_TOKEN (do not commit it)}"
export STATION_E2E=1
python3 -m unittest discover -s "$PROJECT/e2e" -p 'live_ha.py' -v
if [[ -n "${HERMES_API_KEY:-}" ]]; then
  python3 -m unittest discover -s "$PROJECT/e2e" -p 'live_hermes.py' -v
else
  echo "SKIP Hermes E2E: HERMES_API_KEY is not set"
fi
