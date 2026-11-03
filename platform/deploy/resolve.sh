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
#   3. secrets/local-tools/home-deploy in this checkout
#   4. secrets/local-tools/home-deploy in the main checkout that owns the shared
#      Git directory: a linked worktree has no (git-ignored) secrets/ directory
#   5. bazel run //platform/deploy/cmd/home-deploy --, when Bazel is available
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

# home_deploy_main_root prints the directory of the main checkout that owns the
# shared Git directory, so a linked worktree can still find secrets/local-tools.
home_deploy_main_root() {
  local common
  common=$(git -C "$1" rev-parse --path-format=absolute --git-common-dir 2>/dev/null) || return 1
  [ -n "$common" ] || return 1
  dirname "$common"
}

# home_deploy_bazel prints the Bazel binary to use (BAZELISK, PATH, or the
# git-ignored local tool) or fails.
home_deploy_bazel() {
  local root candidate main_root
  if [ -n "${BAZELISK:-}" ]; then
    printf '%s\n' "$BAZELISK"
    return 0
  fi
  if type -P bazel >/dev/null 2>&1; then
    type -P bazel
    return 0
  fi
  root=$(home_deploy_root)
  candidate="$root/secrets/local-tools/bazelisk"
  if [ -x "$candidate" ]; then
    printf '%s\n' "$candidate"
    return 0
  fi
  main_root=$(home_deploy_main_root "$root" 2>/dev/null) || main_root=
  if [ -n "$main_root" ] && [ -x "$main_root/secrets/local-tools/bazelisk" ]; then
    printf '%s\n' "$main_root/secrets/local-tools/bazelisk"
    return 0
  fi
  return 1
}

# home_deploy runs the tool with the given arguments.
home_deploy() {
  local root candidate main_root bazel
  if [ -n "${HOME_DEPLOY:-}" ]; then
    "$HOME_DEPLOY" "$@"
    return $?
  fi
  if command -v home-deploy >/dev/null 2>&1; then
    home-deploy "$@"
    return $?
  fi

  root=$(home_deploy_root)
  for candidate in \
    "$root/secrets/local-tools/home-deploy" \
    "$root/bazel-bin/platform/deploy/cmd/home-deploy/home-deploy_/home-deploy"; do
    if [ -x "$candidate" ]; then
      "$candidate" "$@"
      return $?
    fi
  done

  main_root=$(home_deploy_main_root "$root" 2>/dev/null) || main_root=
  if [ -n "$main_root" ] && [ -x "$main_root/secrets/local-tools/home-deploy" ]; then
    "$main_root/secrets/local-tools/home-deploy" "$@"
    return $?
  fi

  if bazel=$(home_deploy_bazel); then
    ( cd "$root" && "$bazel" run //platform/deploy/cmd/home-deploy -- "$@" )
    return $?
  fi

  echo "home-deploy not found: build it with" >&2
  echo "  bazel build //platform/deploy/cmd/home-deploy --platforms=@rules_go//go/toolchain:linux_amd64" >&2
  echo "and install it under secrets/local-tools/, or set HOME_DEPLOY to its path" >&2
  return 127
}
