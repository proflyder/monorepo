#!/usr/bin/env bash
#MISE description="shellcheck (severity error) over harness, CI and deploy scripts"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT"
exec shellcheck -S error \
  platform/mise-tasks/*.sh \
  platform/mise-tasks/*/*.sh \
  platform/quality/*.sh \
  platform/ci/*.sh \
  platform/deploy/*.sh \
  projects/home/kubernetes/scheduler/*.sh \
  projects/home/kubernetes/scheduler/nft/*.sh \
  projects/home/kubernetes/currency-bot/nft/*.sh \
  projects/station/scripts/*.sh \
  solutions/windows-logon-provider/*.sh \
  platform/workstation/env.sh \
  platform/workstation/run.sh \
  platform/workstation/fnox.sh \
  platform/workstation/with-kube-tunnel.sh \
  platform/workstation/bin/bazel \
  platform/workstation/bin/vault
