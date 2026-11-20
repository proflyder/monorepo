#!/usr/bin/env bash
# shellcheck shell=bash

PREBUILT_DIGESTS_NORMALIZED=""

prebuilt_digest() {
  local name=$1 entry
  [[ -n "$PREBUILT_DIGESTS_NORMALIZED" ]] || return 0
  while IFS= read -r entry; do
    [[ -z "$entry" ]] && continue
    if [[ "$entry" == "$name="* ]]; then
      printf '%s\n' "${entry#*=}"
      return 0
    fi
  done <<<"$PREBUILT_DIGESTS_NORMALIZED"
}

prebuilt_digests_init() {
  local raw="" line="" key="" digest=""
  if [[ -n "${PREBUILT_DIGESTS_FILE:-}" ]]; then
    if [[ ! -f "$PREBUILT_DIGESTS_FILE" ]]; then
      echo "missing PREBUILT_DIGESTS_FILE: $PREBUILT_DIGESTS_FILE" >&2
      return 1
    fi
    raw+=$(<"$PREBUILT_DIGESTS_FILE")$'\n'
  fi
  if [[ -n "${PREBUILT_DIGESTS:-}" ]]; then
    # Intentional: comma-separated pairs share the line-oriented format below.
    raw+="${PREBUILT_DIGESTS//,/$'\n'}"$'\n'
  fi
  [[ -n "$raw" ]] || return 0

  while IFS= read -r line || [[ -n "$line" ]]; do
    line=${line%%#*}
    line="${line#"${line%%[![:space:]]*}"}"
    line="${line%"${line##*[![:space:]]}"}"
    [[ -z "$line" ]] && continue
    key=${line%%=*}
    digest=${line#*=}
    if [[ "$line" != *=* || ! "$key" =~ ^[a-z0-9][a-z0-9._-]*$ ]]; then
      echo "malformed PREBUILT_DIGESTS entry: '$line' (expected <image>=sha256:<hex>)" >&2
      return 1
    fi
    if [[ ! "$digest" =~ ^sha256:[0-9a-f]{64}$ ]]; then
      echo "malformed prebuilt digest for '$key': '$digest'" >&2
      return 1
    fi
    if [[ -n "$(prebuilt_digest "$key")" ]]; then
      echo "duplicate prebuilt digest for '$key'" >&2
      return 1
    fi
    PREBUILT_DIGESTS_NORMALIZED+="${key}=${digest}"$'\n'
  done <<<"$raw"
}

# `set -e` in the sourcing script aborts the deploy on a malformed value.
prebuilt_digests_init
