#!/usr/bin/env bash
# Deploy the durable scheduler and the Home Assistant executors to the home K3s
# cluster as digest-pinned container images.
#
# Mode handling is explicit and idempotent:
#   (default)   build/push images and roll the selected workloads; PRESERVE the
#               bridge/control execute modes and replica counts exactly as they are
#   --shadow    force bridge dry-run: BRIDGE_EXECUTE=false, replicas=0
#   --execute   run cutover preflight, then BRIDGE_EXECUTE=true, replicas=1
#
# Re-running without --execute/--shadow must never stop a live executor.
#
# Images:
#   * built hermetically with Bazel (`//projects/home/kubernetes/scheduler:*_image`)
#     from the Go binaries and a digest-pinned distroless base;
#   * referenced by digest only (`ghcr.io/proflyder/<name>@sha256:...`), never by
#     a moving tag, so a rollback is an exact image;
#   * PUSH_IMAGES=1 pushes them to GHCR with `docker` (CI); otherwise they are
#     imported into the node's containerd over SSH (manual workstation flow).
#
# Prerequisites (External Secrets Operator, values never live in Git):
#   Secret/scheduler-auth and Secret/ha-bridge-ha are materialised by the
#   ExternalSecrets in manifests/external-secrets.yaml from OpenBao
#   secret/k8s/home-assistant/{scheduler-auth,ha-bridge-ha}. Apply them with
#   projects/home/kubernetes/external-secrets/deploy.sh. This script only
#   verifies that they are present and Ready; it never writes their values.
#
# Environment:
#   PRIVATE_ENV     private values file (default secrets/home/scheduler.env)
#   ONLY            comma list of groups: scheduler, ha-bridge, ha-control, all
#   PUSH_IMAGES=1   push new images to GHCR with docker before deploying
#   IMAGE_TAG       git tag/commit used for the pushed tags (default HEAD sha)
#   NODE_SSH        ssh target used to import images when PUSH_IMAGES != 1
#   SSH_KEY/SSH_OPTS  ssh transport for NODE_SSH
#   PREFLIGHT_IN_CLUSTER=1  run the cutover preflight as an in-cluster Job
#   HA_TOKEN_FILE, SCHEDULER_TOKEN_FILE  local preflight tokens
#   SCHEDULER_IMAGE/HA_BRIDGE_IMAGE/HA_CONTROL_IMAGE  pre-resolved refs
#                   (skip building that image; used by the regression test)
#   SKIP_FIREWALL=1 no longer needed: the NodePort filter is host configuration
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
ONLY=${ONLY:-all}
PUSH_IMAGES=${PUSH_IMAGES:-0}
# Registry namespace for PUSH_IMAGES. Overridable so the push path can be
# exercised against a throwaway local registry.
IMAGE_REGISTRY=${IMAGE_REGISTRY:-"ghcr.io/proflyder"}
# Stop after building/pushing (used by the local push/digest verification).
PUSH_ONLY=${PUSH_ONLY:-0}
PREFLIGHT_IN_CLUSTER=${PREFLIGHT_IN_CLUSTER:-0}
SCHEDULER_TOKEN_FILE=${SCHEDULER_TOKEN_FILE:-"$ROOT/secrets/home/scheduler.token"}
OCI="$ROOT/platform/ci/oci_image.py"
PREFLIGHT_JOB=scheduler-preflight
# Pre-deploy state for a safe rollback: deploy.sh records each Deployment it is
# about to change and marks it only after a successful apply. CI restores from
# this, never with a blind `kubectl rollout undo`.
DEPLOY_STATE_DIR=${DEPLOY_STATE_DIR:-"$RENDER_DIR/state"}
# Pinned python image for the in-cluster preflight Job (digest of the multi-arch
# `3.13-slim` index). The node resolves the linux/amd64 child itself.
PREFLIGHT_IMAGE=${PREFLIGHT_IMAGE:-"docker.io/library/python:3.13-slim@sha256:2d44738ebf4ce5f63b9d9ea871e9c7cfdc0426f7b0c97cac915fd88ded1d86bb"}

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

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) BUILD=false ;;
    --shadow) MODE=shadow ;;
    --execute) MODE=execute ;;
    --control-catalog) CONTROL_MODE=catalog ;;
    --control-execute) CONTROL_MODE=execute ;;
    --only) ONLY=$2; shift ;;
    --only=*) ONLY=${1#*=} ;;
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

WANT_SCHEDULER=false
WANT_BRIDGE=false
WANT_CONTROL=false
case "$ONLY" in
  all) WANT_SCHEDULER=true; WANT_BRIDGE=true; WANT_CONTROL=true ;;
  *)
    IFS=',' read -r -a groups <<<"$ONLY"
    for group in "${groups[@]}"; do
      case "$group" in
        scheduler) WANT_SCHEDULER=true ;;
        # The bridge unit owns ha-control as well: they share the HA executor
        # code and are rolled together.
        ha-bridge) WANT_BRIDGE=true; WANT_CONTROL=true ;;
        ha-control) WANT_CONTROL=true ;;
        "") ;;
        *) echo "unknown --only group: $group" >&2; exit 2 ;;
      esac
    done
    ;;
esac
$WANT_SCHEDULER || $WANT_BRIDGE || $WANT_CONTROL || {
  echo "--only selected no workload" >&2
  exit 2
}
[[ "$MODE" == preserve || "$WANT_BRIDGE" == true ]] || {
  echo "--shadow/--execute require the ha-bridge group" >&2
  exit 2
}
[[ "$CONTROL_MODE" == preserve || "$WANT_CONTROL" == true ]] || {
  echo "--control-* requires the ha-control/ha-bridge group" >&2
  exit 2
}

env_value() {
  python3 - "$PRIVATE_ENV" "$1" <<'PY'
import sys, pathlib
path, key = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
for line in path.read_text().splitlines():
    line = line.strip()
    if line and not line.startswith("#") and line.split("=", 1)[0] == key:
        print(line.split("=", 1)[1].strip().strip('"').strip("'"))
        break
PY
}

# Not named `bazel`: a function of that name would shadow the binary and
# `command -v bazel` would find the function itself.
bazel_bin() {
  if [[ -n "${BAZELISK:-}" ]]; then echo "$BAZELISK"; return; fi
  if type -P bazel >/dev/null 2>&1; then type -P bazel; return; fi
  if [[ -x "$ROOT/secrets/local-tools/bazelisk" ]]; then echo "$ROOT/secrets/local-tools/bazelisk"; return; fi
  echo "bazel/bazelisk not found; set BAZELISK" >&2
  exit 1
}

if [[ -z "${IMAGE_TAG:-}" ]]; then
  IMAGE_TAG=$(cd "$ROOT" && git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)
fi

if $BUILD; then
  echo "== building container images with Bazel"
  ( cd "$ROOT" && "$(bazel_bin)" build \
      //projects/home/kubernetes/scheduler:images \
      --platforms=@rules_go//go/toolchain:linux_amd64 )
fi

# Plain variables instead of an associative array: macOS ships bash 3.2 and the
# manual flow must run there too.
IMAGE_REF_SCHEDULER=
IMAGE_REF_HA_BRIDGE=
IMAGE_REF_HA_CONTROL=
set_image_ref() {
  case "$1" in
    scheduler) IMAGE_REF_SCHEDULER=$2 ;;
    ha-bridge) IMAGE_REF_HA_BRIDGE=$2 ;;
    ha-control) IMAGE_REF_HA_CONTROL=$2 ;;
  esac
}
get_image_ref() {
  case "$1" in
    scheduler) echo "$IMAGE_REF_SCHEDULER" ;;
    ha-bridge) echo "$IMAGE_REF_HA_BRIDGE" ;;
    ha-control) echo "$IMAGE_REF_HA_CONTROL" ;;
  esac
}
image_var() {
  case "$1" in
    scheduler) echo SCHEDULER_IMAGE ;;
    ha-bridge) echo HA_BRIDGE_IMAGE ;;
    ha-control) echo HA_CONTROL_IMAGE ;;
  esac
}

push_tag() {
  local name=$1 digest=$2 tag=$3 repo="${IMAGE_REGISTRY}/${name}"
  local bin out pushed
  bin=$(bazel_bin)
  out=$( cd "$ROOT" && "$bin" run "//projects/home/kubernetes/scheduler:${name}_push" \
    --platforms=@rules_go//go/toolchain:linux_amd64 \
    -- --repository "${IMAGE_REGISTRY}/${name}" --tag "$tag" 2>&1 ) || {
      printf '%s\n' "$out" >&2
      echo "pushing ${repo}:${tag} failed" >&2
      return 1
    }
  # crane prints `digest: sha256:…` for a new upload and `existing manifest:
  # …@sha256:…` when the registry already has it; both end with `repo@sha256:…`.
  pushed=$(printf '%s\n' "$out" | grep -oE 'sha256:[0-9a-f]{64}' | tail -1)
  if [[ -z "$pushed" ]]; then
    printf '%s\n' "$out" >&2
    echo "could not read the pushed digest for ${repo}:${tag}" >&2
    return 1
  fi
  if [[ "$pushed" != "$digest" ]]; then
    echo "pushed digest ${repo}@${pushed} does not match the built digest ${repo}@${digest}" >&2
    return 1
  fi
}

resolve_image() {
  local name=$1 var override layout digest tag repo
  var=$(image_var "$name")
  override=${!var:-}
  layout="$ROOT/bazel-bin/projects/home/kubernetes/scheduler/${name}_image"

  if [[ -n "$override" ]]; then
    set_image_ref "$name" "$override"
    echo "== image ${name}: ${override} (pre-resolved)"
    return
  fi
  [[ -d "$layout" ]] || {
    echo "missing image layout $layout; build it or set $var" >&2
    exit 1
  }
  digest=$(python3 "$OCI" digest "$layout")

  if [[ "$PUSH_IMAGES" == 1 ]]; then
    repo="${IMAGE_REGISTRY}/${name}"
    # Push the exact OCI manifest we built with `rules_oci`/crane. A `docker
    # push` rewrites the manifest into Docker media types (and can add an
    # attestation index), so the registry digest would not be the digest we
    # built. Crane stores the bytes as-is: the tag digest must equal $digest.
    push_tag "$name" "$digest" "$IMAGE_TAG" || exit 1
    if [[ "$IMAGE_TAG" != develop ]]; then
      push_tag "$name" "$digest" develop || exit 1
    fi
    echo "== image ${name}: pushed ${repo}@${digest} (tags ${IMAGE_TAG}, develop)"
  elif [[ -n "${NODE_SSH:-}" ]]; then
    python3 "$OCI" pack "$layout" --ref "${IMAGE_REGISTRY}/${name}:${IMAGE_TAG}" \
      --out "$TMP_IMAGES/${name}.tar" >/dev/null
    remote_scp -q "$TMP_IMAGES/${name}.tar" "$NODE_SSH:/tmp/${name}-${IMAGE_TAG}.tar"
    remote_ssh "sudo -n k3s ctr -n k8s.io images import '/tmp/${name}-${IMAGE_TAG}.tar' >/dev/null && rm -f '/tmp/${name}-${IMAGE_TAG}.tar'"
    # The manifest references the digest, so the containerd image record must
    # carry exactly that digest as a name (no registry pull is attempted).
    remote_ssh "sudo -n k3s ctr -n k8s.io images tag '${IMAGE_REGISTRY}/${name}:${IMAGE_TAG}' '${IMAGE_REGISTRY}/${name}@${digest}' >/dev/null"
    echo "== image ${name}: imported into the node as ${IMAGE_REGISTRY}/${name}@${digest}"
  else
    echo "no delivery channel for ${name}: set NODE_SSH (import into the node) or PUSH_IMAGES=1 (push to GHCR)" >&2
    exit 1
  fi
  set_image_ref "$name" "${IMAGE_REGISTRY}/${name}@${digest}"
}

TMP_IMAGES=$(mktemp -d)
trap 'rm -rf "$TMP_IMAGES"' EXIT
# All three images are always resolved: the manifests are rendered as one set
# and a skipped workload still needs a valid reference for its template.
for image in scheduler ha-bridge ha-control; do
  resolve_image "$image"
done

if [[ "$PUSH_ONLY" == 1 ]]; then
  echo "push-only: $(get_image_ref scheduler) $(get_image_ref ha-bridge) $(get_image_ref ha-control)"
  exit 0
fi

IMAGE_FINGERPRINT="$(get_image_ref scheduler)$(get_image_ref ha-bridge)$(get_image_ref ha-control)"
BUILD_ID="$(cd "$ROOT" && git rev-parse --short HEAD 2>/dev/null || echo nogit)-$(printf '%s' "$IMAGE_FINGERPRINT" | python3 -c 'import hashlib,sys; print(hashlib.sha256(sys.stdin.read().encode()).hexdigest()[:12])')"
echo "== build id: $BUILD_ID"

echo "== rendering manifests into $RENDER_DIR"
python3 "$DIR/render.py" --env "$PRIVATE_ENV" --out "$RENDER_DIR" \
  --set "BUILD_ID=$BUILD_ID" \
  --set "SCHEDULER_IMAGE=$(get_image_ref scheduler)" \
  --set "HA_BRIDGE_IMAGE=$(get_image_ref ha-bridge)" \
  --set "HA_CONTROL_IMAGE=$(get_image_ref ha-control)" \
  --set "PREFLIGHT_IMAGE=$PREFLIGHT_IMAGE" \
  "$DIR/manifests/scheduler.yaml" "$DIR/manifests/bridge.yaml" \
  "$DIR/manifests/control.yaml" "$DIR/manifests/control-service.yaml" \
  "$DIR/manifests/preflight-job.yaml"

# --- rollback bookkeeping ---------------------------------------------------
rm -rf "$DEPLOY_STATE_DIR"
mkdir -p "$DEPLOY_STATE_DIR"

record_before() {
  local name=$1
  if $KUBECTL -n "$NS" get deployment "$name" >/dev/null 2>&1; then
    $KUBECTL -n "$NS" get deployment "$name" -o json >"$DEPLOY_STATE_DIR/$name.before.json"
  else
    : >"$DEPLOY_STATE_DIR/$name.absent"
  fi
  rm -f "$DEPLOY_STATE_DIR/$name.applied"
}

mark_applied() {
  : >"$DEPLOY_STATE_DIR/$1.applied"
}

# Replace the live Deployment with the rendered object instead of patching it.
# A patch never removes a volume/volumeMount that an earlier ad-hoc change added;
# a full replace converges the live spec to the manifest and prunes that drift.
# Runtime mode/scaling that the manifest does not own is carried over explicitly.
converge_deployment() {
  local name=$1 file=$2
  shift 2
  python3 "$DIR/converge.py" \
    --kubectl "$KUBECTL" --namespace "$NS" --name "$name" --file "$file" "$@"
}

echo "== checking required secrets"
for s in scheduler-auth ha-bridge-ha; do
  if ! $KUBECTL -n "$NS" get secret "$s" >/dev/null 2>&1; then
    echo "missing secret $NS/$s; apply the ExternalSecrets with" >&2
    echo "  projects/home/kubernetes/external-secrets/deploy.sh" >&2
    exit 1
  fi
  # An ExternalSecret that exists but is not Ready will not repair a deleted or
  # drifted Secret, so fail before touching live workloads.
  if $KUBECTL -n "$NS" get externalsecret "$s" >/dev/null 2>&1; then
    synced=$($KUBECTL -n "$NS" get externalsecret "$s" \
      -o jsonpath='{.status.conditions[?(@.type=="Ready")].status}' 2>/dev/null || true)
    if [[ "$synced" != "True" ]]; then
      echo "ExternalSecret $NS/$s is not Ready (Ready=$synced)" >&2
      exit 1
    fi
  fi
done

if $WANT_SCHEDULER; then
  echo "== applying scheduler"
  record_before scheduler
  converge_deployment scheduler "$RENDER_DIR/scheduler.yaml" --preserve-replicas
  mark_applied scheduler
  $KUBECTL -n "$NS" rollout status deployment/scheduler --timeout=180s
  $KUBECTL -n "$NS" exec deployment/scheduler -- /usr/local/bin/scheduler -healthcheck
  echo "scheduler authenticated readiness: ok"
fi

get_status() {
  $KUBECTL -n "$NS" exec deploy/scheduler -- /usr/local/bin/scheduler -status
}

if $WANT_CONTROL; then
  echo "== ha-control"
  CONTROL_EXISTS=false
  if $KUBECTL -n "$NS" get deployment ha-control >/dev/null 2>&1; then
    CONTROL_EXISTS=true
  fi

  record_before ha-control
  if ! $CONTROL_EXISTS; then
    echo "== first install: applying ha-control (catalog only, 1 replica)"
    $KUBECTL apply -f "$RENDER_DIR/control.yaml"
  else
    echo "== existing ha-control: preserving execute mode/replicas, converging build $BUILD_ID"
    converge_deployment ha-control "$RENDER_DIR/control.yaml" --preserve-env --preserve-replicas
  fi
  mark_applied ha-control
  $KUBECTL apply -f "$RENDER_DIR/control-service.yaml"

  control_queue_pending() {
    # Fail closed: --control-execute refuses when the dedicated control queue
    # has pending or leased events. GetStatus only reports queues with backlog.
    local status
    status=$(get_status)
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
    $KUBECTL -n "$NS" exec deployment/ha-control -- /usr/local/bin/ha-control -healthcheck
    echo "ha-control authenticated readiness: ok"
  fi
fi

if $WANT_BRIDGE; then
  BRIDGE_EXISTS=false
  if $KUBECTL -n "$NS" get deployment ha-bridge >/dev/null 2>&1; then
    BRIDGE_EXISTS=true
  fi

  record_before ha-bridge
  if ! $BRIDGE_EXISTS; then
    echo "== first install: applying ha-bridge at 0 replicas"
    $KUBECTL apply -f "$RENDER_DIR/bridge.yaml"
  else
    # Preserve mode and replicas. The rendered image/annotation replace rolls the
    # pod onto the new immutable image and also prunes any drifted spec fields.
    echo "== existing ha-bridge: preserving mode/replicas, converging build $BUILD_ID"
    converge_deployment ha-bridge "$RENDER_DIR/bridge.yaml" --preserve-env --preserve-replicas
    if [[ "$($KUBECTL -n "$NS" get deployment ha-bridge -o jsonpath='{.spec.replicas}')" != "0" ]]; then
      $KUBECTL -n "$NS" rollout status deployment/ha-bridge --timeout=180s
    fi
  fi
  mark_applied ha-bridge

  if [[ "$MODE" == "shadow" ]]; then
    echo "== switching bridge to shadow (dry-run, 0 replicas)"
    $KUBECTL -n "$NS" set env deployment/ha-bridge BRIDGE_EXECUTE=false
    $KUBECTL -n "$NS" scale deployment/ha-bridge --replicas=0
  fi
fi

preflight_local() {
  local ha_url scheduler_url
  ha_url=$(env_value HA_URL)
  scheduler_url=$(env_value SCHEDULER_PUBLIC_URL)
  python3 "$DIR/preflight.py" \
    --ha-url "$ha_url" \
    --ha-token-file "$HA_TOKEN_FILE" \
    --scheduler-url "$scheduler_url" \
    --scheduler-token-file "$SCHEDULER_TOKEN_FILE" \
    --expected-endpoint "$scheduler_url"
}

preflight_cluster() {
  echo "== creating the in-cluster preflight ConfigMap"
  $KUBECTL -n "$NS" create configmap "$PREFLIGHT_JOB" \
    --from-file=preflight.py="$DIR/preflight.py" \
    --dry-run=client -o yaml | $KUBECTL apply -f -
  $KUBECTL -n "$NS" delete job "$PREFLIGHT_JOB" --ignore-not-found >/dev/null
  $KUBECTL apply -f "$RENDER_DIR/preflight-job.yaml" >/dev/null
  local rc=0
  $KUBECTL -n "$NS" wait --for=condition=complete "job/$PREFLIGHT_JOB" --timeout=180s || rc=$?
  $KUBECTL -n "$NS" logs "job/$PREFLIGHT_JOB" || true
  $KUBECTL -n "$NS" delete job "$PREFLIGHT_JOB" --ignore-not-found >/dev/null
  [[ $rc == 0 ]] || { echo "preflight failed inside the cluster" >&2; exit 1; }
}

preflight() {
  if [[ "$PREFLIGHT_IN_CLUSTER" == "1" ]]; then
    preflight_cluster
  else
    preflight_local
  fi
}

if [[ "$MODE" == "execute" ]]; then
  if [[ "$PREFLIGHT_IN_CLUSTER" != "1" ]]; then
    [[ -n "${HA_TOKEN_FILE:-}" ]] || { echo "--execute requires HA_TOKEN_FILE for preflight" >&2; exit 1; }
    [[ -f "$HA_TOKEN_FILE" ]] || { echo "missing HA_TOKEN_FILE: $HA_TOKEN_FILE" >&2; exit 1; }
    [[ -f "$SCHEDULER_TOKEN_FILE" ]] || { echo "missing SCHEDULER_TOKEN_FILE: $SCHEDULER_TOKEN_FILE" >&2; exit 1; }
  fi
  echo "== cutover preflight"
  preflight
  echo "== starting long-lived executor"
  $KUBECTL -n "$NS" set env deployment/ha-bridge BRIDGE_EXECUTE=true
  $KUBECTL -n "$NS" scale deployment/ha-bridge --replicas=1
  $KUBECTL -n "$NS" rollout status deployment/ha-bridge --timeout=180s
  $KUBECTL -n "$NS" exec deployment/ha-bridge -- /usr/local/bin/ha-bridge -healthcheck
  echo "ha-bridge authenticated readiness: ok"
fi

echo
echo "== pods"
$KUBECTL -n "$NS" get pods -l app.kubernetes.io/name=scheduler -o wide
$KUBECTL -n "$NS" get pods -l app.kubernetes.io/name=ha-bridge -o wide
echo
echo "build: $BUILD_ID  mode: $MODE  only: $ONLY"
