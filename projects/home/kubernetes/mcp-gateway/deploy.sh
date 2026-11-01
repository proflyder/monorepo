#!/usr/bin/env bash
# Deploy the MCP infrastructure gateway to the home K3s cluster.
#
# The gateway image is built hermetically by Bazel from solutions/mcp-gateway
# and referenced by digest only. Secrets are owned by ESO/OpenBao; this script
# only verifies that they are present and Ready and never writes their values.
#
# Environment:
#   PRIVATE_ENV    private values file (default secrets/home/mcp-gateway.env)
#                  keys: LAN_CIDR, NODE_LAN_IP
#   PUSH_IMAGES=1  push the image to GHCR (CI path)
#   NODE_SSH       import the image into the node's containerd (manual path)
#   IMAGE_TAG      tag used for the push/import (default HEAD sha)
#   MCP_GATEWAY_IMAGE  pre-resolved digest reference (skips build/push)
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

LAN_CIDR=$(env_value LAN_CIDR)
NODE_LAN_IP=$(env_value NODE_LAN_IP)
# The NetworkPolicy admits Hermes (hostNetwork -> node address) and kubelet
# probes explicitly, so both values are required.
[[ -n "$LAN_CIDR" && -n "$NODE_LAN_IP" ]] || {
  echo "LAN_CIDR and NODE_LAN_IP are required in $PRIVATE_ENV" >&2
  exit 1
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
  echo "== building the mcp-gateway image with Bazel"
  ( cd "$ROOT" && "$(bazel_bin)" build \
      //solutions/mcp-gateway:mcp-gateway_image \
      --platforms=@rules_go//go/toolchain:linux_amd64 )
fi

OCI="$ROOT/platform/ci/oci_image.py"
IMAGE_REF=${MCP_GATEWAY_IMAGE:-}
if [[ -z "$IMAGE_REF" ]]; then
  layout="$ROOT/bazel-bin/solutions/mcp-gateway/mcp-gateway_image"
  [[ -d "$layout" ]] || { echo "missing image layout $layout; build it first" >&2; exit 1; }
  digest=$(python3 "$OCI" digest "$layout")
  repo="${IMAGE_REGISTRY}/mcp-gateway"
  if [[ "$PUSH_IMAGES" == 1 ]]; then
    out=$( cd "$ROOT" && "$(bazel_bin)" run //solutions/mcp-gateway:mcp-gateway_push \
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
    echo "== image mcp-gateway: pushed ${repo}@${digest} (tag ${IMAGE_TAG})"
  elif [[ -n "${NODE_SSH:-}" ]]; then
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    python3 "$OCI" pack "$layout" --ref "${repo}:${IMAGE_TAG}" --out "$tmp/mcp-gateway.tar" >/dev/null
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

BUILD_ID="$(cd "$ROOT" && git rev-parse --short HEAD 2>/dev/null || echo nogit)-$(printf '%s' "$IMAGE_REF" | python3 -c 'import hashlib,sys; print(hashlib.sha256(sys.stdin.read().encode()).hexdigest()[:12])')"
echo "== build id: $BUILD_ID"

echo "== rendering manifests into $RENDER_DIR"
python3 "$DIR/render.py" \
  --env "$PRIVATE_ENV" --out "$RENDER_DIR" \
  --set "BUILD_ID=$BUILD_ID" \
  --set "MCP_GATEWAY_IMAGE=$IMAGE_REF" \
  "$DIR/manifests/deployment.yaml" \
  "$DIR/manifests/network-policies.yaml" \
  "$DIR/manifests/servicemonitor.yaml"

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

# The gateway's ServiceAccount, Roles and both ExternalSecrets are admin/ESO
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
for es in mcp-gateway-auth mcp-gateway-telegram; do
  if ! $KUBECTL -n "$NS" get externalsecret "$es" >/dev/null 2>&1; then
    echo "missing ExternalSecret $NS/$es; apply it with" >&2
    echo "  kubectl apply -f projects/home/kubernetes/mcp-gateway/manifests/external-secrets.yaml" >&2
    exit 1
  fi
  $KUBECTL -n "$NS" wait --for=condition=Ready "externalsecret/$es" --timeout=120s
done
for secret in mcp-gateway-auth mcp-gateway-telegram; do
  if ! $KUBECTL -n "$NS" get secret "$secret" >/dev/null 2>&1; then
    echo "missing secret $NS/$secret; apply the ExternalSecrets and wait for Ready" >&2
    exit 1
  fi
done

echo "== catalog ConfigMap"
# The catalog is applied from the tracked file (no placeholders in it), so the
# ConfigMap content is exactly the reviewed YAML.
$KUBECTL -n "$NS" create configmap mcp-gateway-catalog \
  --from-file=catalog.yaml="$DIR/manifests/catalog.yaml" \
  --dry-run=client -o yaml | $KUBECTL apply -f -

echo "== NetworkPolicies and ServiceMonitor"
$KUBECTL apply -f "$RENDER_DIR/network-policies.yaml"
$KUBECTL apply -f "$RENDER_DIR/servicemonitor.yaml"

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

mark_applied() { : >"$DEPLOY_STATE_DIR/$1.applied"; }

converge_deployment() {
  local name=$1 file=$2
  python3 "$ROOT/projects/home/kubernetes/scheduler/converge.py" \
    --kubectl "$KUBECTL" --namespace "$NS" --name "$name" --file "$file"
}

echo "== applying mcp-gateway"
record_before mcp-gateway
converge_deployment mcp-gateway "$RENDER_DIR/deployment.yaml"
mark_applied mcp-gateway
$KUBECTL -n "$NS" rollout status deployment/mcp-gateway --timeout=180s

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
