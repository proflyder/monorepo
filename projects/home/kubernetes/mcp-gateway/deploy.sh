#!/usr/bin/env bash
# Deploy the MCP infrastructure gateway to the home K3s cluster.
#
# The gateway image is built hermetically by Bazel from solutions/mcp-gateway
# and referenced by digest only. Secrets are owned by ESO/OpenBao; this script
# only verifies that they are present and Ready and never writes their values.
#
# Environment:
#   PRIVATE_ENV    private values file (default secrets/home/mcp-gateway.env)
#                  keys: LAN_CIDR, NODE_LAN_IP, HAOS_IP, STATION_IP, BB_CT_IP,
#                  BB_PROJECT_STATION_LAB, BB_PROJECT_ENGLISH_NOTES,
#                  BB_DISPATCHER_THREAD_ID
#   PUSH_IMAGES=1  push the image to GHCR (CI path)
#   NODE_SSH       import the image into the node's containerd (manual path)
#   IMAGE_TAG      tag used for the push/import (default HEAD sha)
#   MCP_GATEWAY_IMAGE  pre-resolved digest reference (skips build/push)
#   PREBUILT_DIGESTS / PREBUILT_DIGESTS_FILE  verified digests promoted from the
#                  pull-request build (DECISIONS 36); MCP_GATEWAY_IMAGE wins
#   DEPLOY_STATE_DIR   rollback bookkeeping directory
#   KUBECTL        kubectl binary (default kubectl)
#   ONLY           CI passes the unit workload list; only `mcp-gateway`
#                  (or `all`) is accepted.
#
# One-time prerequisites (admin, not CI):
#   kubectl apply -f projects/home/kubernetes/mcp-gateway/manifests/rbac.yaml
#   kubectl apply -f projects/home/kubernetes/mcp-gateway/ci/rbac.yaml
#   kubectl apply -f projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml
#
# The deploy is idempotent: re-running rolls the Deployment only when the image
# digest/annotation changed and re-applies the same support objects.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck source=platform/ci/prebuilt-digests.sh
source "$ROOT/platform/ci/prebuilt-digests.sh"
# shellcheck source=platform/deploy/resolve.sh
source "$ROOT/platform/deploy/resolve.sh"

NS=home-assistant
DIR="$ROOT/projects/home/kubernetes/mcp-gateway"
KUBECTL=${KUBECTL:-kubectl}
PRIVATE_ENV=${PRIVATE_ENV:-"$ROOT/secrets/home/mcp-gateway.env"}
RENDER_DIR=${RENDER_DIR:-"$ROOT/secrets/local/rendered/mcp-gateway"}
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

if [[ -n "${ONLY:-}" && "$ONLY" != "all" && ",${ONLY}," != *",mcp-gateway,"* ]]; then
  echo "ONLY=$ONLY selects no mcp-gateway workload" >&2
  exit 2
fi

[[ -f "$PRIVATE_ENV" ]] || {
  echo "missing private values file: $PRIVATE_ENV" >&2
  echo "copy mcp-gateway.env.example and fill real values (it is git-ignored)" >&2
  exit 1
}

env_value() {
  home_deploy env get "$PRIVATE_ENV" "$1"
}

LAN_CIDR=$(env_value LAN_CIDR)
NODE_LAN_IP=$(env_value NODE_LAN_IP)
HAOS_IP=$(env_value HAOS_IP)
STATION_IP=$(env_value STATION_IP)
BB_CT_IP=$(env_value BB_CT_IP)
BB_PROJECT_STATION_LAB=$(env_value BB_PROJECT_STATION_LAB)
BB_PROJECT_ENGLISH_NOTES=$(env_value BB_PROJECT_ENGLISH_NOTES)
BB_DISPATCHER_THREAD_ID=$(env_value BB_DISPATCHER_THREAD_ID)
# The NetworkPolicy admits Hermes (hostNetwork -> node address) and kubelet
# probes explicitly; the external checks need the HAOS and station host
# addresses, and the bb group needs the bridge host and the personal projects.
[[ -n "$LAN_CIDR" && -n "$NODE_LAN_IP" && -n "$HAOS_IP" && -n "$STATION_IP" ]] || {
  echo "LAN_CIDR, NODE_LAN_IP, HAOS_IP and STATION_IP are required in $PRIVATE_ENV" >&2
  exit 1
}
[[ -n "$BB_CT_IP" && -n "$BB_PROJECT_STATION_LAB" && -n "$BB_PROJECT_ENGLISH_NOTES" && -n "$BB_DISPATCHER_THREAD_ID" ]] || {
  echo "BB_CT_IP, BB_PROJECT_STATION_LAB, BB_PROJECT_ENGLISH_NOTES and BB_DISPATCHER_THREAD_ID are required in $PRIVATE_ENV" >&2
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
# build (DECISIONS 36). An explicit MCP_GATEWAY_IMAGE stays authoritative.
prebuilt=$(prebuilt_digest mcp-gateway)
if [[ -z "${MCP_GATEWAY_IMAGE:-}" && -n "$prebuilt" ]]; then
  MCP_GATEWAY_IMAGE="${IMAGE_REGISTRY}/mcp-gateway@${prebuilt}"
  echo "== image mcp-gateway: prebuilt ${MCP_GATEWAY_IMAGE}"
fi

if $BUILD && [[ -z "${MCP_GATEWAY_IMAGE:-}" ]]; then
  echo "== building the mcp-gateway image with Bazel"
  # The image target packages the static musl binary through its own Bazel
  # transition, so no Go platform flag is needed (the Go implementation is gone).
  ( cd "$ROOT" && "$(bazel_bin)" build //solutions/mcp-gateway:mcp-gateway_image )
fi

IMAGE_REF=${MCP_GATEWAY_IMAGE:-}
if [[ -z "$IMAGE_REF" ]]; then
  layout="$ROOT/bazel-bin/solutions/mcp-gateway/mcp-gateway_image"
  [[ -d "$layout" ]] || { echo "missing image layout $layout; build it first" >&2; exit 1; }
  digest=$(home_deploy oci digest "$layout")
  repo="${IMAGE_REGISTRY}/mcp-gateway"
  if [[ "$PUSH_IMAGES" == 1 ]]; then
    out=$( cd "$ROOT" && "$(bazel_bin)" run //solutions/mcp-gateway:mcp-gateway_push \
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
    echo "== image mcp-gateway: pushed ${repo}@${digest} (tag ${IMAGE_TAG})"
  elif [[ -n "${NODE_SSH:-}" ]]; then
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    home_deploy oci pack "$layout" --ref "${repo}:${IMAGE_TAG}" --out "$tmp/mcp-gateway.tar" >/dev/null
    scp -q "$tmp/mcp-gateway.tar" "$NODE_SSH:/tmp/mcp-gateway-${IMAGE_TAG}.tar"
    # shellcheck disable=SC2029 # deliberately expanded locally into the remote command
    ssh "$NODE_SSH" "sudo -n k3s ctr -n k8s.io images import '/tmp/mcp-gateway-${IMAGE_TAG}.tar' >/dev/null && rm -f '/tmp/mcp-gateway-${IMAGE_TAG}.tar'"
    # shellcheck disable=SC2029
    ssh "$NODE_SSH" "sudo -n k3s ctr -n k8s.io images tag '${repo}:${IMAGE_TAG}' '${repo}@${digest}' >/dev/null"
    echo "== image mcp-gateway: imported into the node as ${repo}@${digest}"
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
home_deploy render --unit mcp-gateway \
  --env "$PRIVATE_ENV" --out "$RENDER_DIR" \
  --set "BUILD_ID=$BUILD_ID" \
  --rollout-key ROLLOUT_ID \
  --set "MCP_GATEWAY_IMAGE=$IMAGE_REF" \
  --set "BB_CT_IP=$BB_CT_IP" \
  --set "BB_PROJECT_STATION_LAB=$BB_PROJECT_STATION_LAB" \
  --set "BB_PROJECT_ENGLISH_NOTES=$BB_PROJECT_ENGLISH_NOTES" \
  --set "BB_DISPATCHER_THREAD_ID=$BB_DISPATCHER_THREAD_ID" \
  "$DIR/manifests/deployment.yaml" \
  "$DIR/manifests/network-policies.yaml" \
  "$DIR/manifests/servicemonitor.yaml" \
  "$DIR/manifests/catalog.yaml" \
  "$DIR/manifests/bb-targets.yaml"

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

# The gateway's ServiceAccount, Roles and all ExternalSecrets are admin/ESO
# owned. Fail before touching workloads if the one-time apply was skipped.
# CI's ServiceAccount may not even read ServiceAccounts, so Forbidden means
# "admin-managed, cannot check" and a definite NotFound is the hard failure.
if sa_out=$($KUBECTL -n "$NS" get serviceaccount mcp-gateway 2>&1); then
  echo "$sa_out"
elif grep -q "NotFound" <<<"$sa_out"; then
  echo "missing ServiceAccount $NS/mcp-gateway; apply the admin manifests with" >&2
  echo "  kubectl apply -f projects/home/kubernetes/mcp-gateway/manifests/rbac.yaml" >&2
  exit 1
elif grep -q "Forbidden" <<<"$sa_out"; then
  echo "cannot read ServiceAccounts (CI scope); assuming admin-managed $NS/mcp-gateway"
else
  echo "ServiceAccount check failed: $sa_out" >&2
  exit 1
fi

echo "== verifying ExternalSecrets (ESO-owned; CI never applies them)"
for es in mcp-gateway-auth mcp-gateway-telegram mcp-gateway-ha mcp-gateway-bb-ssh mcp-gateway-voice; do
  if ! $KUBECTL -n "$NS" get externalsecret "$es" >/dev/null 2>&1; then
    echo "missing ExternalSecret $NS/$es; apply it with" >&2
    echo "  kubectl apply -f projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml" >&2
    exit 1
  fi
  $KUBECTL -n "$NS" wait --for=condition=Ready "externalsecret/$es" --timeout=120s
done
for secret in mcp-gateway-auth mcp-gateway-telegram mcp-gateway-ha mcp-gateway-bb-ssh mcp-gateway-voice; do
  if ! $KUBECTL -n "$NS" get secret "$secret" >/dev/null 2>&1; then
    echo "missing secret $NS/$secret; apply the ExternalSecrets and wait for Ready" >&2
    exit 1
  fi
done

echo "== catalog and bb-targets ConfigMaps"
# External check hosts and the personal project id are rendered from the
# private env, so the ConfigMaps are built from the rendered copies.
# `home-deploy render` refuses an unknown or unset placeholder, which keeps a
# half-rendered manifest out of the cluster.
$KUBECTL -n "$NS" create configmap mcp-gateway-catalog \
  --from-file=catalog.yaml="$RENDER_DIR/catalog.yaml" \
  --from-file=bb-targets.yaml="$RENDER_DIR/bb-targets.yaml" \
  --dry-run=client -o yaml | $KUBECTL apply -f -

echo "== NetworkPolicies and ServiceMonitor"
$KUBECTL apply -f "$RENDER_DIR/network-policies.yaml"
$KUBECTL apply -f "$RENDER_DIR/servicemonitor.yaml"

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

converge_deployment() {
  local name=$1 file=$2
  home_deploy converge \
    --kubectl "$KUBECTL" --namespace "$NS" --name "$name" --file "$file" \
    --summary-file "${DEPLOY_SUMMARY_FILE:-}"
}

echo "== applying mcp-gateway"
record_before mcp-gateway
converge_deployment mcp-gateway "$RENDER_DIR/deployment.yaml"
mark_applied mcp-gateway
$KUBECTL -n "$NS" rollout status deployment/mcp-gateway --timeout=600s

# The image is distroless; -healthcheck loads the mounted configuration (token,
# catalog, groups) and calls /healthz on loopback, so a broken mount or token
# fails the deploy instead of silently serving 401s.
$KUBECTL -n "$NS" exec deployment/mcp-gateway -- /usr/local/bin/mcp-gateway -healthcheck
echo "mcp-gateway configuration and readiness: ok"

echo
echo "== pods"
$KUBECTL -n "$NS" get pods -l app.kubernetes.io/name=mcp-gateway -o wide
echo
echo "build: $BUILD_ID  namespace: $NS"
