#!/usr/bin/env bash
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
cp "$root/packages/proto/home/v1/text.proto" "$scratch/proto/"
cp "$root/packages/proto/home/v1/alarms.proto" "$scratch/proto/"
cp "$root/packages/proto/home/v1/scenario.proto" "$scratch/proto/"
cp "$root/packages/proto/home/v1/intercom.proto" "$scratch/proto/"

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
        .files(&[
            "scheduler.proto",
            "text.proto",
            "alarms.proto",
            "scenario.proto",
            "intercom.proto",
        ])
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

perl -0pi -e 's/include!\(concat!\(env!\("OUT_DIR"\), "\/home\.v1\.mod\.rs"\)\);/include!("home.v1.mod.rs");/' "$out/mod.rs"
perl -0pi -e 's/^pub mod home/#[rustfmt::skip]\n#[allow(clippy::all, clippy::pedantic, clippy::nursery)]\npub mod home/m' "$out/mod.rs"

if [[ "$mode" == "--check" ]]; then
  if ! diff -r "$generated" "$out" >/dev/null; then
    echo "generate.sh: generated sources differ from src/generated" >&2
    diff -r "$generated" "$out" | head -40 >&2
    exit 1
  fi
  echo "generate.sh: src/generated is up to date"
fi
