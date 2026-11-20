#!/usr/bin/env bash
set -euo pipefail
"$1" "$2" "$3"
"$1" --self-test "$2"
