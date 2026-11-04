#!/usr/bin/env bash
# Regenerate crates/provider/src/bindings.rs and
# crates/cp-selftest/src/bindings.rs from the filters in each crate's
# bindings.txt. Generation is manual and not part of the build.
#
# windows-bindgen 0.63.0 is pinned because it emits code for windows-core
# 0.62.x (`Error::from_thread`) and is installed in a temporary project, not in
# the workspace.
set -euo pipefail

cd "$(dirname "$0")"

BINDGEN_VERSION=0.63.0
OUT_DIRS=("provider" "cp-selftest")

tmp="$(mktemp -d "${TMPDIR:-/tmp}/logon-bindgen.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/src"
cat > "$tmp/Cargo.toml" <<EOF
[package]
name = "logon-bindgen"
version = "0.0.0"
edition = "2021"

[dependencies]
windows-bindgen = "=$BINDGEN_VERSION"
EOF
cat > "$tmp/src/main.rs" <<'EOF'
fn main() {
    let _ = windows_bindgen::bindgen(std::env::args().skip(1));
}
EOF

cargo build --release --quiet --manifest-path "$tmp/Cargo.toml"
bindgen="$tmp/target/release/logon-bindgen"

for crate in "${OUT_DIRS[@]}"; do
  filters=()
  while IFS= read -r line; do
    [[ -z "$line" || "$line" == \#* ]] && continue
    filters+=("$line")
  done < "crates/$crate/bindings.txt"

  "$bindgen" \
    --out "crates/$crate/src/bindings.rs" \
    --no-comment \
    --implement \
    --filter "${filters[@]}"
  rustfmt --edition 2024 "crates/$crate/src/bindings.rs"
  echo "regenerated crates/$crate/src/bindings.rs"
done
