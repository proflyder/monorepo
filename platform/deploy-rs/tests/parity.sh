#!/bin/sh
set -eu

FROZEN=$(dirname "$3")
exec "$1" "$2" "$FROZEN"
