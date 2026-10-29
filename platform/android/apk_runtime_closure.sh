#!/usr/bin/env bash
set -euo pipefail
# Parse real DEX class_defs, then prove the check rejects a class that remains
# referenced but has no definition.
python3 "$1" "$2" "$3"
python3 "$1" --self-test "$2"
