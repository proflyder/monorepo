#!/usr/bin/env bash
# Resolve and run the `home-deploy` Go CLI from the deploy scripts.
#
# Source this file after ROOT is known:
#
#   # shellcheck source=platform/deploy/resolve.sh
#   source "$ROOT/platform/deploy/resolve.sh"
#   home_deploy render --unit scheduler --env "$PRIVATE_ENV" --out "$RENDER_DIR" ...
#
# Resolution order:
#   1. $HOME_DEPLOY (set by CI after one `bazel build` per job, or by a caller)
#   2. `home-deploy` on PATH (an installed/operator binary)
#   3. bazel-bin output of an earlier `bazel build` in this checkout
#   4. bazel run //platform/deploy/cmd/home-deploy --, when Bazel is available
#      (Bazelisk comes from mise: `mise install`, see mise.toml)
#
# Every step is optional, so the manual macOS workstation flow and the CI runner
# use the same scripts. Compatible with macOS bash 3.2 (no associative arrays).
#
# shellcheck shell=bash

# home_deploy_root prints the repository root that contains this file.
home_deploy_root() {
  local source=${BASH_SOURCE[0]:-}
  ( cd "$(dirname "$source")/../.." && pwd )
}

# home_deploy_bazel prints the Bazel binary to use (BAZELISK, or bazel/bazelisk
# on PATH — mise provides both) or fails.
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

# home_deploy runs the tool with the given arguments.
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
  candidate="$root/bazel-bin/platform/deploy/cmd/home-deploy/home-deploy_/home-deploy"
  if [ -x "$candidate" ]; then
    "$candidate" "$@"
    return $?
  fi

  if bazel=$(home_deploy_bazel); then
    ( cd "$root" && "$bazel" run //platform/deploy/cmd/home-deploy -- "$@" )
    return $?
  fi

  echo "home-deploy not found: build it with" >&2
  echo "  bazel build //platform/deploy/cmd/home-deploy --platforms=@rules_go//go/toolchain:linux_amd64" >&2
  echo "or install the tools with \`mise install\` (Bazelisk), or set HOME_DEPLOY" >&2
  return 127
}
