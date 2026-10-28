#!/usr/bin/env bash
# Run the hermetic suite, collect machine-measured coverage where possible,
# and regenerate reports/TEST-COVERAGE.md.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
mkdir -p reports

echo "== hermetic test suite =="
./platform/quality/run-tests.sh

echo "== Go coverage: BLE gateway =="
(cd projects/station/ble-gateway && go test -coverprofile="$ROOT/reports/ble-gateway.cover" ./...)

echo "== Go coverage: Pulsar installer =="
(cd projects/station/upstream/pulsar-rust/installer && GOWORK=off go test -coverprofile="$ROOT/reports/pulsar-installer.cover" ./...)

if command -v coverage >/dev/null 2>&1; then
  echo "== Python line coverage: pairing core =="
  (cd projects/station/resources/quasar-connect-bot && coverage erase && \
    coverage run --source=pairing -m unittest discover -s tests -p 'test_*.py' >/tmp/pairing-cover-tests.out 2>&1 && \
    tail -6 /tmp/pairing-cover-tests.out && \
    coverage json -o "$ROOT/reports/pairing-coverage.json" && \
    coverage report --fail-under=85)
else
  echo "WARN: coverage.py is not installed; skipping pairing line coverage" >&2
fi

python3 platform/quality/report_coverage.py
echo "written: reports/TEST-COVERAGE.md"
