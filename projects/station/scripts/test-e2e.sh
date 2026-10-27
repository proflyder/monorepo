#!/usr/bin/env bash
set -euo pipefail
PROJECT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
: "${STATION_CONTROL_TOKEN:?set STATION_CONTROL_TOKEN (do not commit it)}"
export STATION_E2E=1
python3 -m unittest discover -s "$PROJECT/e2e" -p 'live_gateway.py' -v
