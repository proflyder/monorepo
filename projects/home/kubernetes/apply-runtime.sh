#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"
: "${RUNTIME_PRIVATE_VALUES:?Set RUNTIME_PRIVATE_VALUES to the private runtime values}"
# Existing data directories and local images must exist. For initial migration,
# stop each previous writer before increasing that workload's replica count.
helm upgrade --install runtime "$ROOT/projects/home-assistant/deploy/chart" \
  -n home-assistant -f "$RUNTIME_PRIVATE_VALUES" --wait --timeout 8m
