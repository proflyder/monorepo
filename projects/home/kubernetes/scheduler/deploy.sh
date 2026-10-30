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
#   SCHEDULER_TOKEN_FILE  scheduler token file (default secrets/home/scheduler.token)
#   BAZELISK        path to bazelisk/bazel
#   SSH_KEY         private key for NODE_SSH (optional; uses ssh-agent otherwise)
#   SSH_OPTS        extra ssh/scp options, word-split (optional)
#   REMOTE_STAGE    when set, stage binaries through the fixed root helper
#                   `sudo -n /usr/local/sbin/ricardo-ci-deploy` (CI mode)
#   PREFLIGHT_REMOTE=1  run preflight on the K3s node instead of this host
#   SKIP_FIREWALL=1 skip node firewall installation
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
NS=home-assistant
DIR="$ROOT/projects/home/kubernetes/scheduler"
KUBECTL=${KUBECTL:-kubectl}
PRIVATE_ENV=${PRIVATE_ENV:-"$ROOT/secrets/home/scheduler.env"}
RENDER_DIR="$ROOT/secrets/local/rendered/scheduler"
MODE=preserve
CONTROL_MODE=preserve
BUILD=true
SCHEDULER_TOKEN_FILE=${SCHEDULER_TOKEN_FILE:-"$ROOT/secrets/home/scheduler.token"}

# ssh/scp transport. CI passes a dedicated key and IdentityFile-only use; the
# manual flow keeps using ssh-agent.
SSH_CMD=(ssh)
SCP_CMD=(scp)
if [[ -n "${SSH_KEY:-}" ]]; then
  SSH_CMD+=(-i "$SSH_KEY" -o IdentitiesOnly=yes)
  SCP_CMD+=(-i "$SSH_KEY" -o IdentitiesOnly=yes)
fi
if [[ -n "${SSH_OPTS:-}" ]]; then
  # Intentional word splitting: SSH_OPTS is a list of options, not one argument.
  # shellcheck disable=SC2206
  SSH_CMD+=($SSH_OPTS)
  # shellcheck disable=SC2206
  SCP_CMD+=($SSH_OPTS)
fi

remote_ssh() { "${SSH_CMD[@]}" "$NODE_SSH" "$@"; }
remote_scp() { "${SCP_CMD[@]}" "$@"; }
CI_DEPLOY=/usr/local/sbin/ricardo-ci-deploy

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) BUILD=false ;;
    --shadow) MODE=shadow ;;
    --execute) MODE=execute ;;
    --control-catalog) CONTROL_MODE=catalog ;;
    --control-execute) CONTROL_MODE=execute ;;
    -h|--help) sed -n '2,/^set -euo pipefail/p' "$0"; exit 0 ;;
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

# Portable short release digest (sha256sum and shasum are not both available
# on macOS/Linux). Hashes the file contents in argument order.
release_digest() {
  python3 - "$@" <<'PY'
import hashlib, sys

digest = hashlib.sha256()
for path in sys.argv[1:]:
    with open(path, "rb") as handle:
        digest.update(handle.read())
print(digest.hexdigest()[:12])
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
      //solutions/ha-bridge/cmd/ha-control \
      --platforms=@rules_go//go/toolchain:linux_amd64 )
  mkdir -p "$RELEASE_DIR"
  cp -f "$ROOT/bazel-bin/solutions/scheduler/cmd/scheduler/scheduler_/scheduler" "$RELEASE_DIR/scheduler"
  cp -f "$ROOT/bazel-bin/solutions/ha-bridge/cmd/ha-bridge/ha-bridge_/ha-bridge" "$RELEASE_DIR/ha-bridge"
  cp -f "$ROOT/bazel-bin/solutions/ha-bridge/cmd/ha-control/ha-control_/ha-control" "$RELEASE_DIR/ha-control"
fi

for f in scheduler ha-bridge ha-control; do
  [[ -x "$RELEASE_DIR/$f" ]] || { echo "missing $RELEASE_DIR/$f" >&2; exit 1; }
done
file "$RELEASE_DIR/scheduler" | grep -q 'ELF 64-bit' || { echo "scheduler is not a linux ELF binary" >&2; exit 1; }

BUILD_ID="$(cd "$ROOT" && git rev-parse --short HEAD 2>/dev/null || echo nogit)-$(release_digest "$RELEASE_DIR/scheduler" "$RELEASE_DIR/ha-bridge" "$RELEASE_DIR/ha-control")"
echo "== build id: $BUILD_ID"
echo "$BUILD_ID" > "$RELEASE_DIR/BUILD_ID"

echo "== rendering manifests into $RENDER_DIR"
python3 "$DIR/render.py" --env "$PRIVATE_ENV" --out "$RENDER_DIR" \
  --set "BUILD_ID=$BUILD_ID" \
  "$DIR/manifests/scheduler.yaml" "$DIR/manifests/bridge.yaml" \
  "$DIR/manifests/control.yaml" "$DIR/manifests/control-service.yaml" \
  "$DIR/nft/scheduler-nodeport.nft"

if [[ -n "${NODE_SSH:-}" && -n "${REMOTE_STAGE:-}" ]]; then
  # CI mode: the deploy account cannot write /opt/ricardo directly. Binaries
  # are copied to its staging directory and installed by the fixed root helper.
  echo "== staging immutable release via $NODE_SSH:$REMOTE_STAGE"
  remote_ssh "rm -rf '$REMOTE_STAGE' && mkdir -p '$REMOTE_STAGE' && chmod 700 '$REMOTE_STAGE'"
  remote_scp -q "$RELEASE_DIR/scheduler" "$RELEASE_DIR/ha-bridge" \
    "$RELEASE_DIR/ha-control" "$NODE_SSH:$REMOTE_STAGE/"
  remote_ssh "sudo -n $CI_DEPLOY stage '$BUILD_ID'"
elif [[ -n "${NODE_SSH:-}" ]]; then
  RELEASE_PATH="/opt/ricardo/releases/$BUILD_ID"
  echo "== staging immutable release on $NODE_SSH:$RELEASE_PATH"
  remote_ssh "install -d -m 0755 '$RELEASE_PATH/bin' /opt/ricardo/releases /opt/ricardo/control/releases"
  remote_scp -q "$RELEASE_DIR/scheduler" "$RELEASE_DIR/ha-bridge" "$NODE_SSH:$RELEASE_PATH/bin/"
  # ha-control has its own immutable tree so rolling it never replaces the
  # running scheduler/ha-bridge binaries.
  remote_ssh "install -d -m 0755 '/opt/ricardo/control/releases/$BUILD_ID/bin'"
  remote_scp -q "$RELEASE_DIR/ha-control" "$NODE_SSH:/opt/ricardo/control/releases/$BUILD_ID/bin/"
  remote_ssh "ln -sfn '$RELEASE_PATH' /opt/ricardo/current.new && mv -Tf /opt/ricardo/current.new /opt/ricardo/current"
  remote_ssh "ln -sfn '/opt/ricardo/control/releases/$BUILD_ID' /opt/ricardo/control/current.new && mv -Tf /opt/ricardo/control/current.new /opt/ricardo/control/current && readlink -f /opt/ricardo/current /opt/ricardo/control/current"
fi

if [[ "${SKIP_FIREWALL:-0}" != "1" ]]; then
  [[ -n "${NODE_SSH:-}" ]] || {
    echo "NODE_SSH is required to install the NodePort source filter before exposure" >&2
    echo "set NODE_SSH or SKIP_FIREWALL=1 when the filter is already installed" >&2
    exit 1
  }
  echo "== installing NodePort source filter on $NODE_SSH"
  if [[ -n "${REMOTE_STAGE:-}" ]]; then
    # The helper renders its own root-owned template; CI never supplies nft
    # text, only validated values.
    remote_ssh "sudo -n $CI_DEPLOY firewall \
      '$(env_value LAN_CIDR)' '$(env_value NODE_LAN_IP)' \
      '$(env_value SCHEDULER_NODE_PORT)' '$(env_value CONTROL_NODE_PORT)'"
  else
    remote_scp -q "$RENDER_DIR/scheduler-nodeport.nft" "$NODE_SSH:/etc/ricardo-scheduler-nodeport.nft"
    remote_ssh 'nft delete table inet ricardo_scheduler 2>/dev/null || true; nft -f /etc/ricardo-scheduler-nodeport.nft && nft list table inet ricardo_scheduler >/dev/null'
  fi
fi

echo "== checking required secrets"
for s in scheduler-auth ha-bridge-ha; do
  if ! $KUBECTL -n "$NS" get secret "$s" >/dev/null 2>&1; then
    echo "missing secret $NS/$s; create it with --from-file (see header)" >&2
    exit 1
  fi
done

echo "== applying scheduler"
$KUBECTL apply -f "$RENDER_DIR/scheduler.yaml"
$KUBECTL -n "$NS" rollout status deployment/scheduler --timeout=180s
$KUBECTL -n "$NS" exec deployment/scheduler -- /opt/ricardo/bin/scheduler -healthcheck
echo "scheduler authenticated readiness: ok"

echo "== ha-control"
CONTROL_EXISTS=false
if $KUBECTL -n "$NS" get deployment ha-control >/dev/null 2>&1; then
  CONTROL_EXISTS=true
fi

if ! $CONTROL_EXISTS; then
  echo "== first install: applying ha-control (catalog only, 1 replica)"
  $KUBECTL apply -f "$RENDER_DIR/control.yaml"
else
  echo "== existing ha-control: preserving execute mode/replicas, rolling build $BUILD_ID"
  $KUBECTL -n "$NS" patch deployment/ha-control --type merge -p \
    "{\"spec\":{\"template\":{\"metadata\":{\"annotations\":{\"ricardo.dev/build\":\"$BUILD_ID\"}}}}}"
fi
$KUBECTL apply -f "$RENDER_DIR/control-service.yaml"

control_queue_pending() {
  # Fail closed: --control-execute refuses when the dedicated control queue has
  # pending or leased events. GetStatus only reports queues with backlog.
  local status
  status=$($KUBECTL -n "$NS" exec deploy/scheduler -- sh -c \
    'wget -qO- --post-data={} --header="Content-Type: application/json" --header="Authorization: Bearer $(cat /run/secrets/scheduler/token)" http://127.0.0.1:8098/ricardo.v1.SchedulerService/GetStatus')
  printf '%s' "$status" | python3 -c '
import json,sys

def fail(msg):
    print("control preflight refused:", msg, file=sys.stderr)
    sys.exit(1)

try:
    d = json.load(sys.stdin)
except Exception as exc:
    fail(f"GetStatus is not JSON: {exc}")
if not isinstance(d, dict):
    fail("GetStatus response is not an object")
if d.get("ready") is not True:
    fail("scheduler reports ready=false")
queues = d.get("queues")
if not isinstance(queues, list) or not queues:
    # GetStatus always reports the default queues; an empty/absent list means a
    # malformed or stale response, not an empty control queue.
    fail("GetStatus has no queues array")
total = 0
for q in queues:
    if not isinstance(q, dict) or not isinstance(q.get("name"), str):
        fail(f"malformed queue entry: {q!r}")
    for key in ("pending", "leased"):
        value = q.get(key, 0)
        if isinstance(value, bool) or not isinstance(value, int) or value < 0:
            fail("malformed %s for queue %r: %r" % (key, q.get("name"), value))
        if q["name"] == "control":
            total += value
if total > 0:
    fail(f"control queue is not empty: pending+leased={total}")
print("control queue empty")
'
}

if [[ "$CONTROL_MODE" == "catalog" ]]; then
  echo "== ha-control -> catalog only (execution disabled)"
  $KUBECTL -n "$NS" set env deployment/ha-control CONTROL_EXECUTE=false
fi
if [[ "$CONTROL_MODE" == "execute" ]]; then
  echo "== ha-control cutover preflight"
  control_queue_pending
  $KUBECTL -n "$NS" set env deployment/ha-control CONTROL_EXECUTE=true
  $KUBECTL -n "$NS" scale deployment/ha-control --replicas=1
fi

if [[ "$($KUBECTL -n "$NS" get deployment ha-control -o jsonpath='{.spec.replicas}')" != "0" ]]; then
  $KUBECTL -n "$NS" rollout status deployment/ha-control --timeout=180s
  $KUBECTL -n "$NS" exec deployment/ha-control -- /opt/ricardo/bin/ha-control -healthcheck
  echo "ha-control authenticated readiness: ok"
fi

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

preflight() {
  local ha_url scheduler_url remote_dir rc
  ha_url=$(env_value HA_URL)
  scheduler_url=$(env_value SCHEDULER_PUBLIC_URL)
  if [[ "${PREFLIGHT_REMOTE:-0}" == "1" ]]; then
    # The CI runner can reach the K3s node but not HA or the NodePorts. Run the
    # same read-only preflight on the node, where both endpoints are local.
    [[ -n "${NODE_SSH:-}" ]] || { echo "PREFLIGHT_REMOTE requires NODE_SSH" >&2; exit 1; }
    remote_dir=$(remote_ssh 'mktemp -d "$HOME/preflight.XXXXXX"')
    [[ -n "$remote_dir" ]] || { echo "cannot create remote preflight dir" >&2; exit 1; }
    remote_scp -q "$DIR/preflight.py" "$HA_TOKEN_FILE" "$SCHEDULER_TOKEN_FILE" \
      "$NODE_SSH:$remote_dir/"
    rc=0
    remote_ssh "python3 '$remote_dir/preflight.py' \
      --ha-url '$ha_url' \
      --ha-token-file '$remote_dir/$(basename "$HA_TOKEN_FILE")' \
      --scheduler-url '$scheduler_url' \
      --scheduler-token-file '$remote_dir/$(basename "$SCHEDULER_TOKEN_FILE")' \
      --expected-endpoint '$scheduler_url'" || rc=$?
    remote_ssh "rm -rf '$remote_dir'" || true
    [[ $rc == 0 ]] || { echo "preflight failed on $NODE_SSH" >&2; exit 1; }
    return 0
  fi
  python3 "$DIR/preflight.py" \
    --ha-url "$ha_url" \
    --ha-token-file "$HA_TOKEN_FILE" \
    --scheduler-url "$scheduler_url" \
    --scheduler-token-file "$SCHEDULER_TOKEN_FILE" \
    --expected-endpoint "$scheduler_url"
}

if [[ "$MODE" == "execute" ]]; then
  [[ -n "${HA_TOKEN_FILE:-}" ]] || { echo "--execute requires HA_TOKEN_FILE for preflight" >&2; exit 1; }
  [[ -f "$HA_TOKEN_FILE" ]] || { echo "missing HA_TOKEN_FILE: $HA_TOKEN_FILE" >&2; exit 1; }
  [[ -f "$SCHEDULER_TOKEN_FILE" ]] || { echo "missing SCHEDULER_TOKEN_FILE: $SCHEDULER_TOKEN_FILE" >&2; exit 1; }
  echo "== cutover preflight"
  preflight
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
