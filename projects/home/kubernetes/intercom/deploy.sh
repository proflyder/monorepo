#!/usr/bin/env bash
# Deploy the intercom signaling service to the home K3s cluster.
#
# The image is built hermetically by Bazel from solutions/intercom and
# referenced by digest only. Secrets are owned by ESO/OpenBao; this script only
# verifies that they are present and Ready and never writes their values.
#
# Environment:
#   PRIVATE_ENV          private values file (default secrets/home/intercom.env)
#                        key: INTERCOM_NODE_PORT
#   PUSH_IMAGES=1        push the image to GHCR (CI path)
#   NODE_SSH             import the image into the node's containerd (manual path)
#   IMAGE_TAG            tag used for the push/import (default HEAD sha)
#   INTERCOM_IMAGE       pre-resolved digest reference (skips build/push)
#   PREBUILT_DIGESTS / PREBUILT_DIGESTS_FILE
#                        verified digests promoted from the pull-request build
#   DEPLOY_STATE_DIR     rollback bookkeeping directory
#   KUBECTL              kubectl binary (default kubectl)
#   ONLY                 CI passes the unit workload list; only `intercom`
#                        (or `all`) is accepted.
#
# One-time prerequisites (admin, not CI):
#   secret/k8s/home-assistant/intercom in OpenBao with keys phone_token and
#   station_token, then:
#     kubectl apply -f projects/home/kubernetes/intercom/manifests/external-secrets.yaml
#
# The deploy is idempotent: re-running rolls the Deployment only when the image
# digest/annotation changed and re-applies the same Service and IngressRoute.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck source=platform/ci/prebuilt-digests.sh
source "$ROOT/platform/ci/prebuilt-digests.sh"
# shellcheck source=platform/deploy/resolve.sh
source "$ROOT/platform/deploy/resolve.sh"

NS=home-assistant
DIR="$ROOT/projects/home/kubernetes/intercom"
KUBECTL=${KUBECTL:-kubectl}
PRIVATE_ENV=${PRIVATE_ENV:-"$ROOT/secrets/home/intercom.env"}
RENDER_DIR=${RENDER_DIR:-"$ROOT/secrets/local/rendered/intercom"}
DEPLOY_STATE_DIR=${DEPLOY_STATE_DIR:-"$RENDER_DIR/state"}
PUSH_IMAGES=${PUSH_IMAGES:-0}
IMAGE_REGISTRY=${IMAGE_REGISTRY:-"ghcr.io/proflyder"}
BUILD=true
DEPLOY_NO_BUILD=${DEPLOY_NO_BUILD:-0}
if [[ "$DEPLOY_NO_BUILD" == "1" ]]; then
  BUILD=false
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-build) BUILD=false ;;
    -h|--help) sed -n '2,/^set -euo pipefail/p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

if [[ -n "${ONLY:-}" && "$ONLY" != "all" && ",${ONLY}," != *",intercom,"* ]]; then
  echo "ONLY=$ONLY selects no intercom workload" >&2
  exit 2
fi

[[ -f "$PRIVATE_ENV" ]] || {
  echo "missing private values file: $PRIVATE_ENV" >&2
  echo "copy intercom.env.example and fill real values (it is git-ignored)" >&2
  exit 1
}

env_value() {
  home_deploy env get "$PRIVATE_ENV" "$1"
}

INTERCOM_NODE_PORT=$(env_value INTERCOM_NODE_PORT)
[[ "$INTERCOM_NODE_PORT" =~ ^[0-9]+$ ]] && (( 10#$INTERCOM_NODE_PORT >= 30000 && 10#$INTERCOM_NODE_PORT <= 32767 )) || {
  echo "INTERCOM_NODE_PORT must be a NodePort in 30000..32767 in $PRIVATE_ENV" >&2
  exit 1
}
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

prebuilt=$(prebuilt_digest intercom)
if [[ -z "${INTERCOM_IMAGE:-}" && -n "$prebuilt" ]]; then
  INTERCOM_IMAGE="${IMAGE_REGISTRY}/intercom@${prebuilt}"
  echo "== image intercom: prebuilt ${INTERCOM_IMAGE}"
fi

if $BUILD && [[ -z "${INTERCOM_IMAGE:-}" ]]; then
  echo "== building the intercom image with Bazel"
  ( cd "$ROOT" && "$(bazel_bin)" build \
      //solutions/intercom:intercom_image \
      --platforms=//platform/bazel:linux_amd64 )
fi

IMAGE_REF=${INTERCOM_IMAGE:-}
if [[ -z "$IMAGE_REF" && "$DEPLOY_NO_BUILD" == "1" ]]; then
  echo "no promoted digest for intercom: PR CI did not publish it for this tree and" >&2
  echo "DEPLOY_NO_BUILD=1 forbids building it on the deployment runner (DECISIONS 36)." >&2
  exit 1
fi
if [[ -z "$IMAGE_REF" ]]; then
  layout="$ROOT/bazel-bin/solutions/intercom/intercom_image"
  [[ -d "$layout" ]] || { echo "missing image layout $layout; build it first" >&2; exit 1; }
  digest=$(home_deploy oci digest "$layout")
  repo="${IMAGE_REGISTRY}/intercom"
  if [[ "$PUSH_IMAGES" == 1 ]]; then
    out=$( cd "$ROOT" && "$(bazel_bin)" run //solutions/intercom:intercom_push \
      --platforms=//platform/bazel:linux_amd64 \
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
    echo "== image intercom: pushed ${repo}@${digest} (tag ${IMAGE_TAG})"
  elif [[ -n "${NODE_SSH:-}" ]]; then
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    home_deploy oci pack "$layout" --ref "${repo}:${IMAGE_TAG}" --out "$tmp/intercom.tar" >/dev/null
    scp -q "$tmp/intercom.tar" "$NODE_SSH:/tmp/intercom-${IMAGE_TAG}.tar"
    # shellcheck disable=SC2029 # deliberately expanded locally into the remote command
    ssh "$NODE_SSH" "sudo -n k3s ctr -n k8s.io images import '/tmp/intercom-${IMAGE_TAG}.tar' >/dev/null && rm -f '/tmp/intercom-${IMAGE_TAG}.tar'"
    # shellcheck disable=SC2029
    ssh "$NODE_SSH" "sudo -n k3s ctr -n k8s.io images tag '${repo}:${IMAGE_TAG}' '${repo}@${digest}' >/dev/null"
    echo "== image intercom: imported into the node as ${repo}@${digest}"
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
home_deploy render --unit intercom \
  --env "$PRIVATE_ENV" --out "$RENDER_DIR" \
  --set "BUILD_ID=$BUILD_ID" \
  --rollout-key ROLLOUT_ID \
  --set "INTERCOM_IMAGE=$IMAGE_REF" \
  --set "INTERCOM_NODE_PORT=$INTERCOM_NODE_PORT" \
  "$DIR/manifests/intercom.yaml"

echo "== namespace $NS"
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
if ! $KUBECTL -n "$NS" get externalsecret intercom >/dev/null 2>&1; then
  echo "missing ExternalSecret $NS/intercom; apply it with" >&2
  echo "  kubectl apply -f projects/home/kubernetes/intercom/manifests/external-secrets.yaml" >&2
  exit 1
fi
$KUBECTL -n "$NS" wait --for=condition=Ready externalsecret/intercom --timeout=120s
if ! $KUBECTL -n "$NS" get secret intercom >/dev/null 2>&1; then
  echo "missing Secret $NS/intercom; wait for the ExternalSecret to become Ready" >&2
  exit 1
fi

# --- rollback bookkeeping ---------------------------------------------------
rm -rf "$DEPLOY_STATE_DIR"
mkdir -p "$DEPLOY_STATE_DIR"

record_before() {
  home_deploy state record --state-dir "$DEPLOY_STATE_DIR" \
    --namespace "$NS" --name "$1" --kubectl "$KUBECTL"
}

mark_applied() {
  home_deploy state mark --state-dir "$DEPLOY_STATE_DIR" --name "$1"
}

echo "== applying intercom"
record_before intercom
if $KUBECTL -n "$NS" get deployment intercom >/dev/null 2>&1; then
  generation_before=$($KUBECTL -n "$NS" get deployment intercom -o jsonpath='{.metadata.generation}' 2>/dev/null || true)
else
  generation_before=""
fi
$KUBECTL apply -f "$RENDER_DIR/intercom.yaml"
if [[ -n "${DEPLOY_SUMMARY_FILE:-}" ]]; then
  if [[ -z "$generation_before" ]]; then
    status=created
  else
    generation_after=$($KUBECTL -n "$NS" get deployment intercom -o jsonpath='{.metadata.generation}' 2>/dev/null || true)
    if [[ -z "$generation_after" ]]; then
      status=unknown
    elif [[ "$generation_before" == "$generation_after" ]]; then
      status=unchanged
    else
      status=rolled-out
    fi
  fi
  printf '%s\t%s\n' intercom "$status" >> "$DEPLOY_SUMMARY_FILE"
fi
mark_applied intercom
$KUBECTL -n "$NS" rollout status deployment/intercom --timeout=600s

# The image is distroless; -healthcheck loads the mounted tokens and calls
# /healthz on loopback, so a broken Secret mount fails the deploy.
$KUBECTL -n "$NS" exec deployment/intercom -- /usr/local/bin/intercom -healthcheck
echo "intercom configuration and readiness: ok"

echo
echo "== pods"
$KUBECTL -n "$NS" get pods -l app.kubernetes.io/name=intercom -o wide
echo
echo "build: $BUILD_ID  namespace: $NS  nodePort: $INTERCOM_NODE_PORT"
