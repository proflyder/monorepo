#!/bin/bash
# Double-click on macOS. Connection details remain in ignored local configuration.
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$HERE/../../../.." && pwd)"
PYTHON="$ROOT/.venv/bin/python"
[[ -x "$PYTHON" ]] || PYTHON=python3
exec "$PYTHON" "$HERE/launch.py" "$@"
