#!/usr/bin/env bash
# Deploy the durable scheduler and the Home Assistant executor to the home K3s
# cluster.
#
# Mode handling is explicit and idempotent:
#   (default)   install/update binaries and scheduler; PRESERVE the bridge mode
#               and replica count exactly as they are
#   --shadow    force bridge dry-run: BRIDGE_EXECUTE=false, replicas=0
#   --execute   run cutover preflight, then BRIDGE_EXECUTE=true, replicas=1
#
# Re-running without --execute/--shadow must never stop a live executor.
#
# Firewall comes before exposure: the NodePort source filter is installed on the
# node before the Service is applied. Set SKIP_FIREWALL=1 only when the filter
# is already managed elsewhere.
#
# Prerequisites (created once, out of band; values never live in Git):
#   kubectl -n home-assistant create secret generic scheduler-auth \
#     --from-file=token=/path/to/scheduler.token
#   kubectl -n home-assistant create secret generic ha-bridge-ha \
#     --from-file=token=/path/to/ha.token
#
# Environment:
#   PRIVATE_ENV     private values file (default secrets/home/scheduler.env)
#   NODE_SSH        ssh target for binary release + firewall install
#   HA_TOKEN_FILE   HA token file, required for --execute preflight
#   BAZELISK        path to bazelisk/bazel
#   SKIP_FIREWALL=1 skip node firewall installation
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
NS=home-assistant
DIR="$ROOT/projects/home/kubernetes/scheduler"
KUBECTL=${KUBECTL:-kubectl}
PRIVATE_ENV=${PRIVATE_ENV:-"$ROOT/secrets/home/scheduler.env"}
RENDER_DIR="$ROOT/secrets/local/rendered/scheduler"
MODE=preserve
BUILD=true

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) BUILD=false ;;
    --shadow) MODE=shadow ;;
    --execute) MODE=execute ;;
    -h|--help) sed -n '2,32p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

[[ -f "$PRIVATE_ENV" ]] || {
  echo "missing private values file: $PRIVATE_ENV" >&2
  echo "copy scheduler.env.example and fill real values (it is git-ignored)" >&2
  exit 1
}

env_value() {
  python3 - "$PRIVATE_ENV" "$1" <<'PY'
import sys, pathlib
path, key = pathlib.Path(sys.argv[1]), sys.argv[2]
for line in path.read_text().splitlines():
    line = line.strip()
    if line and not line.startswith("#") and line.split("=", 1)[0] == key:
        print(line.split("=", 1)[1].strip().strip('"').strip("'"))
        break
PY
}

bazel() {
  if [[ -n "${BAZELISK:-}" ]]; then echo "$BAZELISK"; return; fi
  if command -v bazel >/dev/null 2>&1; then echo bazel; return; fi
  if [[ -x "$ROOT/secrets/local-tools/bazelisk" ]]; then echo "$ROOT/secrets/local-tools/bazelisk"; return; fi
  echo "bazel/bazelisk not found; set BAZELISK" >&2
  exit 1
}

RELEASE_DIR="$DIR/dist"
if $BUILD; then
  echo "== building static linux/amd64 binaries with Bazel"
  ( cd "$ROOT" && "$(bazel)" build \
      //solutions/scheduler/cmd/scheduler \
      //solutions/ha-bridge/cmd/ha-bridge \
      --platforms=@rules_go//go/toolchain:linux_amd64 )
  mkdir -p "$RELEASE_DIR"
  cp -f "$ROOT/bazel-bin/solutions/scheduler/cmd/scheduler/scheduler_/scheduler" "$RELEASE_DIR/scheduler"
  cp -f "$ROOT/bazel-bin/solutions/ha-bridge/cmd/ha-bridge/ha-bridge_/ha-bridge" "$RELEASE_DIR/ha-bridge"
fi

for f in scheduler ha-bridge; do
  [[ -x "$RELEASE_DIR/$f" ]] || { echo "missing $RELEASE_DIR/$f" >&2; exit 1; }
done
file "$RELEASE_DIR/scheduler" | grep -q 'ELF 64-bit' || { echo "scheduler is not a linux ELF binary" >&2; exit 1; }

BUILD_ID="$(cd "$ROOT" && git rev-parse --short HEAD 2>/dev/null || echo nogit)-$(shasum -a 256 "$RELEASE_DIR/scheduler" "$RELEASE_DIR/ha-bridge" | shasum -a 256 | cut -c1-12)"
echo "== build id: $BUILD_ID"
echo "$BUILD_ID" > "$RELEASE_DIR/BUILD_ID"

echo "== rendering manifests into $RENDER_DIR"
python3 "$DIR/render.py" --env "$PRIVATE_ENV" --out "$RENDER_DIR" \
  --set "BUILD_ID=$BUILD_ID" \
  "$DIR/manifests/scheduler.yaml" "$DIR/manifests/bridge.yaml" \
  "$DIR/nft/scheduler-nodeport.nft" \
  "$DIR/../tailscale/scheduler-ingress.yaml"

if [[ -n "${NODE_SSH:-}" ]]; then
  RELEASE_PATH="/opt/ricardo/releases/$BUILD_ID"
  echo "== staging immutable release on $NODE_SSH:$RELEASE_PATH"
  ssh "$NODE_SSH" "install -d -m 0755 '$RELEASE_PATH/bin' /opt/ricardo/releases"
  scp -q "$RELEASE_DIR/scheduler" "$RELEASE_DIR/ha-bridge" "$NODE_SSH:$RELEASE_PATH/bin/"
  ssh "$NODE_SSH" "ln -sfn '$RELEASE_PATH' /opt/ricardo/current.new && mv -Tf /opt/ricardo/current.new /opt/ricardo/current && readlink -f /opt/ricardo/current"
fi

if [[ "${SKIP_FIREWALL:-0}" != "1" ]]; then
  [[ -n "${NODE_SSH:-}" ]] || {
    echo "NODE_SSH is required to install the NodePort source filter before exposure" >&2
    echo "set NODE_SSH or SKIP_FIREWALL=1 when the filter is already installed" >&2
    exit 1
  }
  echo "== installing NodePort source filter on $NODE_SSH"
  scp -q "$RENDER_DIR/scheduler-nodeport.nft" "$NODE_SSH:/etc/ricardo-scheduler-nodeport.nft"
  ssh "$NODE_SSH" 'nft -f /etc/ricardo-scheduler-nodeport.nft && nft list table inet ricardo_scheduler >/dev/null'
fi

echo "== checking required secrets"
for s in scheduler-auth ha-bridge-ha; do
  if ! $KUBECTL -n "$NS" get secret "$s" >/dev/null 2>&1; then
    echo "missing secret $NS/$s; create it with --from-file (see header)" >&2
    exit 1
  fi
done

echo "== applying scheduler and tailnet route"
$KUBECTL apply -f "$RENDER_DIR/scheduler.yaml"
$KUBECTL apply -f "$RENDER_DIR/scheduler-ingress.yaml"
$KUBECTL -n "$NS" rollout status deployment/scheduler --timeout=180s
$KUBECTL -n "$NS" exec deployment/scheduler -- /opt/ricardo/bin/scheduler -healthcheck
echo "scheduler authenticated readiness: ok"

BRIDGE_EXISTS=false
if $KUBECTL -n "$NS" get deployment ha-bridge >/dev/null 2>&1; then
  BRIDGE_EXISTS=true
fi

if ! $BRIDGE_EXISTS; then
  echo "== first install: applying ha-bridge at 0 replicas"
  $KUBECTL apply -f "$RENDER_DIR/bridge.yaml"
else
  # Preserve mode and replicas. Only the build annotation changes, which rolls
  # the pod onto the new immutable release.
  echo "== existing ha-bridge: preserving mode/replicas, rolling build $BUILD_ID"
  # The build marker lives on the pod template; annotating only the Deployment
  # metadata would not restart pods or pick up the new release.
  $KUBECTL -n "$NS" patch deployment/ha-bridge --type merge -p \
    "{\"spec\":{\"template\":{\"metadata\":{\"annotations\":{\"ricardo.dev/build\":\"$BUILD_ID\"}}}}}"
  if [[ "$($KUBECTL -n "$NS" get deployment ha-bridge -o jsonpath='{.spec.replicas}')" != "0" ]]; then
    $KUBECTL -n "$NS" rollout status deployment/ha-bridge --timeout=180s
  fi
fi

if [[ "$MODE" == "shadow" ]]; then
  echo "== switching bridge to shadow (dry-run, 0 replicas)"
  $KUBECTL -n "$NS" set env deployment/ha-bridge BRIDGE_EXECUTE=false
  $KUBECTL -n "$NS" scale deployment/ha-bridge --replicas=0
fi

if [[ "$MODE" == "execute" ]]; then
  [[ -n "${HA_TOKEN_FILE:-}" ]] || { echo "--execute requires HA_TOKEN_FILE for preflight" >&2; exit 1; }
  [[ -f "$HA_TOKEN_FILE" ]] || { echo "missing HA_TOKEN_FILE: $HA_TOKEN_FILE" >&2; exit 1; }
  echo "== cutover preflight"
  python3 "$DIR/preflight.py" \
    --ha-url "$(env_value HA_URL)" \
    --ha-token-file "$HA_TOKEN_FILE" \
    --scheduler-url "$(env_value SCHEDULER_PUBLIC_URL)" \
    --scheduler-token-file "$ROOT/secrets/home/scheduler.token" \
    --expected-endpoint "$(env_value SCHEDULER_PUBLIC_URL)"
  echo "== starting long-lived executor"
  $KUBECTL -n "$NS" set env deployment/ha-bridge BRIDGE_EXECUTE=true
  $KUBECTL -n "$NS" scale deployment/ha-bridge --replicas=1
  $KUBECTL -n "$NS" rollout status deployment/ha-bridge --timeout=180s
  $KUBECTL -n "$NS" exec deployment/ha-bridge -- /opt/ricardo/bin/ha-bridge -healthcheck
  echo "ha-bridge authenticated readiness: ok"
fi

echo
echo "== pods"
$KUBECTL -n "$NS" get pods -l app.kubernetes.io/name=scheduler -o wide
$KUBECTL -n "$NS" get pods -l app.kubernetes.io/name=ha-bridge -o wide
echo
echo "build: $BUILD_ID  mode: $MODE"
