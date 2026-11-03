#!/usr/bin/env bash
# Prebuilt image digests for the deploy scripts (CI promotion path).
#
# `deploy-home` promotes the images a pull request already built, verified and
# published under `tree-<git tree hash>` tags instead of rebuilding them after
# merge (DECISIONS 36). It passes the verified digests to the existing deploy
# entrypoints:
#
#   PREBUILT_DIGESTS="scheduler=sha256:…,ha-bridge=sha256:…"
#   PREBUILT_DIGESTS_FILE=/path/to/digests   # one `image=sha256:…` per line
#
# Both forms may be combined; a file may contain `#` comments and blank lines.
# The scripts then skip their own build/push for those images and reference the
# digest directly. Local/manual runs set neither variable, so their behaviour is
# unchanged.
#
# Fail closed: a malformed or duplicate entry aborts the deploy at source time.
# A typo must never silently fall back to an unverified image, and a digest that
# was not produced by the promotion step must never be deployed.
#
# Usage (after `ROOT` is known):
#   # shellcheck source=platform/ci/prebuilt-digests.sh
#   source "$ROOT/platform/ci/prebuilt-digests.sh"
#   digest=$(prebuilt_digest scheduler)   # empty when nothing was prebuilt
#
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
