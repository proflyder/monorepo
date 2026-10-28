#!/usr/bin/env bash
# Hermetic tests live beside their owners; this file only orchestrates them.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
for suite in projects/home-assistant/ha-voice projects/home-assistant/hermes projects/home-assistant/stack projects/home-assistant/deploy projects/home/proxmox projects/station/scripts platform/quality; do
  echo "== Python: $suite =="
  python3 -m unittest discover -s "$suite" -p 'test_*.py' -v
done
# Root Go module: reusable solutions and shared packages. Android/Bazel gates are
# separate (they need the Android SDK) and are documented in docs/TESTING.md.
echo "== Go: solutions and packages =="
go test ./solutions/... ./packages/...
(cd projects/station/ble-gateway && go test ./...)
(cd projects/station/upstream/pulsar-rust/installer && GOWORK=off go test ./...)
(cd projects/station/resources/quasar-connect-bot && python3 -m unittest discover -s tests -p 'test_*.py' -v)
echo "ALL HERMETIC TESTS PASSED"
