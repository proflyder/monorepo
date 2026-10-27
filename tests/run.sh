#!/usr/bin/env bash
# Fast, hermetic test suite. Does not touch the live station or HA.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

echo "== Python: HA config, shell, pairing =="
python3 -m unittest discover -s tests -t . -p 'test_*.py' -v

echo "== Go: BLE gateway =="
(cd projects/station/ble-gateway && go test ./...)

echo "== Go: Pulsar installer (upstream tests) =="
(cd projects/station/upstream/pulsar/installer && GOWORK=off go test ./...)

echo "== Python: quasar-connect-bot pairing =="
(cd projects/station/resources/quasar-connect-bot && python3 -m unittest discover -s tests -p 'test_*.py' -v)

echo "ALL HERMETIC TESTS PASSED"
