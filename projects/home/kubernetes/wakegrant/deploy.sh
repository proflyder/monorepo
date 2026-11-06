#!/usr/bin/env bash
# Deploy the wakegrant service to the home K3s cluster.
#
# The image is built hermetically by Bazel from solutions/wakegrant and
# referenced by digest only. Secrets are owned by ESO/OpenBao; this script only
# verifies that they are present and Ready and never writes their values.
#
# Environment:
#   PRIVATE_ENV        private values file (default secrets/home/wakegrant.env)
#                      keys: WAKE_GRANT_NODE_PORT
#   PUSH_IMAGES=1      push the image to GHCR (CI path)
#   NODE_SSH           import the image into the node's containerd (manual path)
#   IMAGE_TAG          tag used for the push/import (default HEAD sha)
#   WAKE_GRANT_IMAGE   pre-resolved digest reference (skips build/push)
#   PREBUILT_DIGESTS / PREBUILT_DIGESTS_FILE  verified digests promoted from the
#                      pull-request build (DECISIONS 36); WAKE_GRANT_IMAGE wins
#   DEPLOY_STATE_DIR   rollback bookkeeping directory
#   KUBECTL            kubectl binary (default kubectl)
#   ONLY               CI passes the unit workload list; only `wakegrant`
#                      (or `all`) is accepted.
#
# One-time prerequisites (admin, not CI):
#   kubectl apply -f projects/home/kubernetes/wakegrant/manifests/external-secrets.yaml
#   (or run projects/home/kubernetes/external-secrets/deploy.sh)
#
# The deploy is idempotent: re-running rolls the Deployment only when the image
# digest/annotation changed and re-applies the same PVC and Service.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck source=platform/ci/prebuilt-digests.sh
source "$ROOT/platform/ci/prebuilt-digests.sh"
# shellcheck source=platform/deploy/resolve.sh
source "$ROOT/platform/deploy/resolve.sh"

NS=home-assistant
DIR="$ROOT/projects/home/kubernetes/wakegrant"
KUBECTL=${KUBECTL:-kubectl}
PRIVATE_ENV=${PRIVATE_ENV:-"$ROOT/secrets/home/wakegrant.env"}
RENDER_DIR=${RENDER_DIR:-"$ROOT/secrets/local/rendered/wakegrant"}
DEPLOY_STATE_DIR=${DEPLOY_STATE_DIR:-"$RENDER_DIR/state"}
PUSH_IMAGES=${PUSH_IMAGES:-0}
IMAGE_REGISTRY=${IMAGE_REGISTRY:-"ghcr.io/proflyder"}
BUILD=true

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) BUILD=false ;;
    -h|--help) sed -n '2,/^set -euo pipefail/p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

if [[ -n "${ONLY:-}" && "$ONLY" != "all" && ",${ONLY}," != *",wakegrant,"* ]]; then
  echo "ONLY=$ONLY selects no wakegrant workload" >&2
  exit 2
fi

[[ -f "$PRIVATE_ENV" ]] || {
  echo "missing private values file: $PRIVATE_ENV" >&2
  echo "copy wakegrant.env.example and fill real values (it is git-ignored)" >&2
  exit 1
}

env_value() {
  home_deploy env get "$PRIVATE_ENV" "$1"
}

WAKE_GRANT_NODE_PORT=$(env_value WAKE_GRANT_NODE_PORT)
[[ "$WAKE_GRANT_NODE_PORT" =~ ^[0-9]+$ ]] && (( 10#$WAKE_GRANT_NODE_PORT >= 30000 && 10#$WAKE_GRANT_NODE_PORT <= 32767 )) || {
  echo "WAKE_GRANT_NODE_PORT must be a NodePort in 30000..32767 in $PRIVATE_ENV" >&2
  exit 1
}

# Not named `bazel`: a function of that name would shadow the binary and
# `command -v bazel` would find the function itself.
bazel_bin() {
  if [[ -n "${BAZELISK:-}" ]]; then echo "$BAZELISK"; return; fi
  if type -P bazel >/dev/null 2>&1; then type -P bazel; return; fi
  if type -P bazelisk >/dev/null 2>&1; then type -P bazelisk; return; fi
  echo "bazel/bazelisk not found; set BAZELISK" >&2
  exit 1
}

if [[ -z "${IMAGE_TAG:-}" ]]; then
  IMAGE_TAG=$(cd "$ROOT" && git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)
fi

# A verified digest promoted from the pull-request build replaces the local
# build (DECISIONS 36). An explicit WAKE_GRANT_IMAGE stays authoritative.
prebuilt=$(prebuilt_digest wakegrant)
if [[ -z "${WAKE_GRANT_IMAGE:-}" && -n "$prebuilt" ]]; then
  WAKE_GRANT_IMAGE="${IMAGE_REGISTRY}/wakegrant@${prebuilt}"
  echo "== image wakegrant: prebuilt ${WAKE_GRANT_IMAGE}"
fi

if $BUILD && [[ -z "${WAKE_GRANT_IMAGE:-}" ]]; then
  echo "== building the wakegrant image with Bazel"
  ( cd "$ROOT" && "$(bazel_bin)" build \
      //solutions/wakegrant:wakegrant_image \
      --platforms=@rules_go//go/toolchain:linux_amd64 )
fi

IMAGE_REF=${WAKE_GRANT_IMAGE:-}
if [[ -z "$IMAGE_REF" ]]; then
  layout="$ROOT/bazel-bin/solutions/wakegrant/wakegrant_image"
  [[ -d "$layout" ]] || { echo "missing image layout $layout; build it first" >&2; exit 1; }
  digest=$(home_deploy oci digest "$layout")
  repo="${IMAGE_REGISTRY}/wakegrant"
  if [[ "$PUSH_IMAGES" == 1 ]]; then
    out=$( cd "$ROOT" && "$(bazel_bin)" run //solutions/wakegrant:wakegrant_push \
      --platforms=@rules_go//go/toolchain:linux_amd64 \
      -- --repository "$repo" --tag "$IMAGE_TAG" 2>&1 ) || {
        printf '%s\n' "$out" >&2
        echo "pushing ${repo}:${IMAGE_TAG} failed" >&2
        exit 1
      }
    pushed=$(printf '%s\n' "$out" | grep -oE 'sha256:[0-9a-f]{64}' | tail -1)
    [[ "$pushed" == "$digest" ]] || {
      printf '%s\n' "$out" >&2
      echo "pushed digest ${pushed:-?} does not match built digest ${digest}" >&2
      exit 1
    }
    echo "== image wakegrant: pushed ${repo}@${digest} (tag ${IMAGE_TAG})"
  elif [[ -n "${NODE_SSH:-}" ]]; then
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    home_deploy oci pack "$layout" --ref "${repo}:${IMAGE_TAG}" --out "$tmp/wakegrant.tar" >/dev/null
    scp -q "$tmp/wakegrant.tar" "$NODE_SSH:/tmp/wakegrant-${IMAGE_TAG}.tar"
    # shellcheck disable=SC2029 # deliberately expanded locally into the remote command
    ssh "$NODE_SSH" "sudo -n k3s ctr -n k8s.io images import '/tmp/wakegrant-${IMAGE_TAG}.tar' >/dev/null && rm -f '/tmp/wakegrant-${IMAGE_TAG}.tar'"
    # shellcheck disable=SC2029
    ssh "$NODE_SSH" "sudo -n k3s ctr -n k8s.io images tag '${repo}:${IMAGE_TAG}' '${repo}@${digest}' >/dev/null"
    echo "== image wakegrant: imported into the node as ${repo}@${digest}"
  else
    echo "no delivery channel: set NODE_SSH (import) or PUSH_IMAGES=1 (push)" >&2
    exit 1
  fi
  IMAGE_REF="${repo}@${digest}"
fi

BUILD_ID="$(home_deploy build-id \
  --revision "$(cd "$ROOT" && git rev-parse --short HEAD 2>/dev/null || echo nogit)" \
  --fingerprint "$IMAGE_REF")"
echo "== build id: $BUILD_ID"

echo "== rendering manifests into $RENDER_DIR"
home_deploy render --unit wakegrant \
  --env "$PRIVATE_ENV" --out "$RENDER_DIR" \
  --set "BUILD_ID=$BUILD_ID" \
  --rollout-key ROLLOUT_ID \
  --set "WAKE_GRANT_IMAGE=$IMAGE_REF" \
  --set "WAKE_GRANT_NODE_PORT=$WAKE_GRANT_NODE_PORT" \
  "$DIR/manifests/wakegrant.yaml"

echo "== namespace $NS"
# The namespace is cluster-scoped and admin-applied. CI's ServiceAccount may
# not read namespaces at all, so only a definite NotFound is an error here.
if ns_out=$($KUBECTL get namespace "$NS" -o name 2>&1); then
  echo "$ns_out exists"
elif grep -q "NotFound" <<<"$ns_out"; then
  echo "namespace $NS is missing; apply projects/home/kubernetes/scheduler/manifests/namespace.yaml as an admin" >&2
  exit 1
elif grep -q "Forbidden" <<<"$ns_out"; then
  echo "cannot read namespaces (CI scope); assuming admin-managed $NS"
else
  echo "namespace check failed: $ns_out" >&2
  exit 1
fi

echo "== verifying the ESO-owned Secret"
if ! $KUBECTL -n "$NS" get externalsecret wakegrant >/dev/null 2>&1; then
  echo "missing ExternalSecret $NS/wakegrant; apply it with" >&2
  echo "  kubectl apply -f projects/home/kubernetes/wakegrant/manifests/external-secrets.yaml" >&2
  exit 1
fi
$KUBECTL -n "$NS" wait --for=condition=Ready externalsecret/wakegrant --timeout=120s
if ! $KUBECTL -n "$NS" get secret wakegrant >/dev/null 2>&1; then
  echo "missing Secret $NS/wakegrant; wait for the ExternalSecret to become Ready" >&2
  exit 1
fi

# --- rollback bookkeeping ---------------------------------------------------
rm -rf "$DEPLOY_STATE_DIR"
mkdir -p "$DEPLOY_STATE_DIR"

# The marker contract (.before.json / .absent / .applied) is owned by
# `home-deploy state`, so a mixed run with an older deploy.sh stays safe.
record_before() {
  home_deploy state record --state-dir "$DEPLOY_STATE_DIR" \
    --namespace "$NS" --name "$1" --kubectl "$KUBECTL"
}

mark_applied() {
  home_deploy state mark --state-dir "$DEPLOY_STATE_DIR" --name "$1"
}

echo "== applying wakegrant"
record_before wakegrant
# The rollout annotation is content-addressed: an identical deploy leaves
# metadata.generation unchanged, so `kubectl apply` creates no new ReplicaSet
# and no pod restart. Record that per workload for the deploy-home summary.
if $KUBECTL -n "$NS" get deployment wakegrant >/dev/null 2>&1; then
  generation_before=$($KUBECTL -n "$NS" get deployment wakegrant -o jsonpath='{.metadata.generation}' 2>/dev/null || true)
else
  generation_before=""
fi
$KUBECTL apply -f "$RENDER_DIR/wakegrant.yaml"
if [[ -n "${DEPLOY_SUMMARY_FILE:-}" ]]; then
  if [[ -z "$generation_before" ]]; then
    status=created
  else
    generation_after=$($KUBECTL -n "$NS" get deployment wakegrant -o jsonpath='{.metadata.generation}' 2>/dev/null || true)
    if [[ -z "$generation_after" ]]; then
      status=unknown
    elif [[ "$generation_before" == "$generation_after" ]]; then
      status=unchanged
    else
      status=rolled-out
    fi
  fi
  printf '%s\t%s\n' wakegrant "$status" >> "$DEPLOY_SUMMARY_FILE"
fi
mark_applied wakegrant
$KUBECTL -n "$NS" rollout status deployment/wakegrant --timeout=600s

# The image is distroless; -healthcheck loads the mounted configuration (token
# and machines) and calls /healthz on loopback, so a broken mount fails the
# deploy instead of silently serving 401s.
$KUBECTL -n "$NS" exec deployment/wakegrant -- /usr/local/bin/wakegrant -healthcheck
echo "wakegrant configuration and readiness: ok"

echo
echo "== pods"
$KUBECTL -n "$NS" get pods -l app.kubernetes.io/name=wakegrant -o wide
echo
echo "build: $BUILD_ID  namespace: $NS  nodePort: $WAKE_GRANT_NODE_PORT"
