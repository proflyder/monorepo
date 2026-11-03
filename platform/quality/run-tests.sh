#!/usr/bin/env bash
# Hermetic tests live beside their owners; this file only orchestrates them.
#
# Optional local sources are ignored checkouts (resources/, nested upstream
# repositories). Their tests run when the source is present and are skipped
# with an explicit reason on a clean checkout or in CI.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

skip() {
  echo "SKIP: $*"
}

for suite in projects/home-assistant/ha-voice projects/home-assistant/hermes projects/home-assistant/stack projects/home-assistant/deploy projects/home/proxmox projects/home/windows-pc solutions/windows-logon-provider projects/home/kubernetes/openbao projects/station/scripts projects/station/tools/voice-recorder projects/station/tools/wakeword-personalize; do
  echo "== Python: $suite =="
  python3 -m unittest discover -s "$suite" -p 'test_*.py' -v
done

# CI/repo-quality glue is dependency-free Node ESM with the built-in runner.
if command -v node >/dev/null 2>&1; then
  echo "== Node: platform/ci and platform/quality =="
  node --test platform/ci/*.test.mjs platform/quality/*.test.mjs
else
  skip "Node.js is not installed: platform CI/quality tests need node --test"
fi

# Python checks that outlive the CI-glue migration: first-party Python product
# syntax/secret gates and the out-of-scope oci_image.py helper tests.
echo "== Python: platform/quality static gate =="
python3 -m unittest discover -s platform/quality -p 'test_static.py' -v
echo "== Python: platform/ci oci helper =="
python3 -m unittest discover -s platform/ci -p 'test_oci_image.py' -v

# Dependency-free edge Workers use Node's built-in runner, not npm.
if command -v node >/dev/null 2>&1; then
  echo "== JS: solutions/heartbeat =="
  node --test solutions/heartbeat/src/*.test.js
else
  skip "Node.js is not installed: solutions/heartbeat tests need node --test"
fi

# Scheduler deployment helpers: real preflight regression and the mocked-kubectl
# control-mode test (the shell test is not covered by unittest discovery).
echo "== Scheduler deployment: preflight =="
(cd projects/home/kubernetes/scheduler && python3 -m unittest test_preflight.py test_rollback.py -v)

# The control-mode regression drives deploy.sh --skip-build with pre-resolved
# image digests, so it needs neither Bazel nor release binaries in dist/.
echo "== Scheduler deployment: deploy control =="
bash projects/home/kubernetes/scheduler/test_deploy_control.sh

# Root Go module: reusable solutions and shared packages. Android/Bazel gates are
# separate (they need the Android SDK) and are documented in docs/TESTING.md.
echo "== Go: solutions and packages =="
go test ./solutions/... ./packages/...
(cd projects/station/ble-gateway && go test ./...)

if [[ -d projects/station/upstream/pulsar-rust/installer ]]; then
  echo "== Go: Pulsar installer (nested checkout) =="
  (cd projects/station/upstream/pulsar-rust/installer && GOWORK=off go test ./...)
else
  skip "Pulsar installer Go tests: projects/station/upstream/pulsar-rust is not checked out"
fi

if [[ -d projects/station/resources/quasar-connect-bot ]]; then
  echo "== Python: quasar-connect-bot (nested checkout) =="
  (cd projects/station/resources/quasar-connect-bot && python3 -m unittest discover -s tests -p 'test_*.py' -v)
else
  skip "quasar-connect-bot tests: projects/station/resources/quasar-connect-bot is not checked out"
fi

echo "ALL HERMETIC TESTS PASSED"
