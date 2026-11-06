#!/usr/bin/env bash
# Regenerate the checked-in home.v1 Rust stubs.
#
# The generated code is committed (like the Android JSON mapping) so a plain
# `cargo build`/`bazel build` needs neither protoc nor a build.rs. This script
# drives connectrpc-build from a throwaway Cargo project because
# connectrpc-build is a build-script library, not a protoc plugin.
#
# Requirements (not committed; install into an ignored tools directory):
#   protoc 33.x                  (PROTOC=/path/to/protoc, else PATH)
#   network on the first run     (connectrpc-build/buffa from crates.io)
#
# Example:
#   PROTOC=/path/to/protoc packages/rust/home-proto/generate.sh
#   packages/rust/home-proto/generate.sh --check   # diff without writing
set -euo pipefail

crate_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
root="$(cd "$crate_dir/../../.." && pwd)"
generated="$crate_dir/src/generated"
mode="${1:-write}"

protoc="${PROTOC:-$(command -v protoc || true)}"
if [[ -z "$protoc" ]]; then
  echo "generate.sh: protoc not found; set PROTOC or add it to PATH" >&2
  exit 1
fi

scratch="$(mktemp -d "${TMPDIR:-/tmp}/home-proto-gen.XXXXXX")"
trap 'rm -rf "$scratch"' EXIT

mkdir -p "$scratch/src" "$scratch/proto"
cp "$root/packages/proto/home/v1/scheduler.proto" "$scratch/proto/"
cp "$root/packages/proto/home/v1/alarms.proto" "$scratch/proto/"

cat > "$scratch/Cargo.toml" <<'EOF'
[package]
name = "home-proto-generate"
version = "0.0.0"
edition = "2024"
publish = false

[build-dependencies]
# Pinned exactly in [workspace.dependencies] of the monorepo; the generator is
# a throwaway project, so it repeats the pins here.
connectrpc-build = "=0.9.0"
EOF

cat > "$scratch/build.rs" <<'EOF'
fn main() {
    let out = std::env::var("GEN_OUT").expect("GEN_OUT");
    connectrpc_build::Config::new()
        .files(&["scheduler.proto", "alarms.proto"])
        .includes(&["proto"])
        .out_dir(&out)
        .include_file("mod.rs")
        .compile()
        .expect("connectrpc-build");
}
EOF

echo 'pub fn placeholder() {}' > "$scratch/src/lib.rs"

if [[ "$mode" == "--check" ]]; then
  out="$scratch/out"
  mkdir -p "$out"
else
  out="$generated"
  rm -rf "$out"
  mkdir -p "$out"
fi

CARGO_TARGET_DIR="$scratch/target" GEN_OUT="$out" PROTOC="$protoc" \
  cargo build --quiet --manifest-path "$scratch/Cargo.toml"

# connectrpc-build emits an `include!` with an absolute OUT_DIR. Commit a
# relative include instead so the stubs work from the crate source tree.
perl -0pi -e 's/include!\(concat!\(env!\("OUT_DIR"\), "\/home\.v1\.mod\.rs"\)\);/include!("home.v1.mod.rs");/' "$out/mod.rs"
# The generated files are formatted by the generator; keep rustfmt from
# rewriting two megabytes on every `cargo fmt --all`.
perl -0pi -e 's/^pub mod home/#[rustfmt::skip]\n#[allow(clippy::all, clippy::pedantic, clippy::nursery)]\npub mod home/m' "$out/mod.rs"

if [[ "$mode" == "--check" ]]; then
  # Compare only the generated sources; the throwaway project leaves no
  # timestamp or build output in the tree.
  if ! diff -r "$generated" "$out" >/dev/null; then
    echo "generate.sh: generated sources differ from src/generated" >&2
    diff -r "$generated" "$out" | head -40 >&2
    exit 1
  fi
  echo "generate.sh: src/generated is up to date"
fi
