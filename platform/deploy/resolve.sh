#!/usr/bin/env bash
# shellcheck shell=bash

home_deploy_root() {
  local source=${BASH_SOURCE[0]:-}
  ( cd "$(dirname "$source")/../.." && pwd )
}

home_deploy_bazel() {
  if [ -n "${BAZELISK:-}" ]; then
    printf '%s\n' "$BAZELISK"
    return 0
  fi
  local candidate
  for candidate in bazel bazelisk; do
    if type -P "$candidate" >/dev/null 2>&1; then
      type -P "$candidate"
      return 0
    fi
  done
  return 1
}

home_deploy() {
  local root bazel candidate
  if [ -n "${HOME_DEPLOY:-}" ]; then
    "$HOME_DEPLOY" "$@"
    return $?
  fi
  if command -v home-deploy >/dev/null 2>&1; then
    home-deploy "$@"
    return $?
  fi

  root=$(home_deploy_root)
  candidate="$root/bazel-bin/platform/deploy-rs/home-deploy"
  if [ -x "$candidate" ]; then
    "$candidate" "$@"
    return $?
  fi

  if [[ "${DEPLOY_NO_BUILD:-0}" == "1" ]]; then
    echo "DEPLOY_NO_BUILD=1: refusing to build/run home-deploy from source in this" >&2
    echo "checkout; deploy-home must pass HOME_DEPLOY extracted from the promoted" >&2
    echo "ghcr.io/proflyder/home-deploy image (DECISIONS 36)" >&2
    return 127
  fi

  if bazel=$(home_deploy_bazel); then
    ( cd "$root" && "$bazel" run //platform/deploy-rs:home-deploy -- "$@" )
    return $?
  fi

  echo "home-deploy not found: build it with" >&2
  echo "  bazel build //platform/deploy-rs:home-deploy" >&2
  echo "or install the tools with \`mise install\` (Bazelisk), or set HOME_DEPLOY" >&2
  return 127
}

