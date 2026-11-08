#!/usr/bin/env bash
#MISE description="Build, release-sign and distribute the Android app via Firebase (signing material from OpenBao; --dry-run)"
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
exec "$ROOT/platform/workstation/fnox.sh" -P android exec -- "$ROOT/projects/mobile/android/distribute.sh" "$@"
