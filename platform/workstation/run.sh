#!/usr/bin/env bash
set -euo pipefail
python="$(mise which python3)"
exec "$python" "$@"
