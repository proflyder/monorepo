#!/usr/bin/env bash
# Run a repository Python tool with the interpreter pinned in mise.toml.
# `mise which` is independent of PATH order: another version manager's shims
# (asdf, pyenv) earlier in PATH must not pick a different python3.
set -euo pipefail
python="$(mise which python3)"
exec "$python" "$@"
