#!/usr/bin/env bash
# Deploy the currency bot and its Cloudflare Tunnel to the home K3s cluster.
#
# The bot image is built hermetically by Bazel from solutions/currency-bot and
# referenced by digest only. Secrets are owned by ESO/OpenBao; this script only
# verifies that they are present and Ready and never writes their values.
#
# Environment:
#   PRIVATE_ENV       private values file (default secrets/home/currency-bot.env)
#                     keys: WEBHOOK_URL, LAN_CIDR, NODE_LAN_IP
#   PUSH_IMAGES=1     push the image to GHCR (CI path)
#   NODE_SSH          import the image into the node's containerd (manual path)
#   IMAGE_TAG         tag used for the push/import (default HEAD sha)
#   CURRENCY_BOT_IMAGE  pre-resolved digest reference (skips build/push)
#   DEPLOY_STATE_DIR  rollback bookkeeping directory
#   KUBECTL           kubectl binary (default kubectl)
#   ONLY              CI passes the unit workload list; only `currency-bot`
#                     (or `all`) is accepted.
#
# The deploy is idempotent: re-running rolls the Deployment only when the image
# digest/annotation changed and re-applies the same support objects.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/projects/home/kubernetes/versions.sh"

NS=currency-bot
DIR="$ROOT/projects/home/kubernetes/currency-bot"
KUBECTL=${KUBECTL:-kubectl}
PRIVATE_ENV=${PRIVATE_ENV:-"$ROOT/secrets/home/currency-bot.env"}
RENDER_DIR=${RENDER_DIR:-"$ROOT/secrets/local/rendered/currency-bot"}
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

if [[ -n "${ONLY:-}" && "$ONLY" != "all" && ",${ONLY}," != *",currency-bot,"* ]]; then
  echo "ONLY=$ONLY selects no currency-bot workload" >&2
  exit 2
fi

[[ -f "$PRIVATE_ENV" ]] || {
  echo "missing private values file: $PRIVATE_ENV" >&2
  echo "copy currency-bot.env.example and fill real values (it is git-ignored)" >&2
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

WEBHOOK_URL=$(env_value WEBHOOK_URL)
LAN_CIDR=$(env_value LAN_CIDR)
NODE_LAN_IP=$(env_value NODE_LAN_IP)
[[ "$WEBHOOK_URL" == https://*/telegram/webhook ]] || {
  echo "WEBHOOK_URL must be https://<public host>/telegram/webhook" >&2
  exit 1
}
[[ -n "$LAN_CIDR" && -n "$NODE_LAN_IP" ]] || {
  echo "LAN_CIDR and NODE_LAN_IP are required in $PRIVATE_ENV" >&2
  exit 1
}

bazel() {
  if [[ -n "${BAZELISK:-}" ]]; then echo "$BAZELISK"; return; fi
  if command -v bazel >/dev/null 2>&1; then echo bazel; return; fi
  if [[ -x "$ROOT/secrets/local-tools/bazelisk" ]]; then echo "$ROOT/secrets/local-tools/bazelisk"; return; fi
  echo "bazel/bazelisk not found; set BAZELISK" >&2
  exit 1
}

if [[ -z "${IMAGE_TAG:-}" ]]; then
  IMAGE_TAG=$(cd "$ROOT" && git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)
fi

if $BUILD; then
  echo "== building the currency-bot image with Bazel"
  ( cd "$ROOT" && "$(bazel)" build \
      //solutions/currency-bot:currency-bot_image \
      --platforms=@rules_go//go/toolchain:linux_amd64 )
fi

OCI="$ROOT/platform/ci/oci_image.py"
IMAGE_REF=${CURRENCY_BOT_IMAGE:-}
if [[ -z "$IMAGE_REF" ]]; then
  layout="$ROOT/bazel-bin/solutions/currency-bot/currency-bot_image"
  [[ -d "$layout" ]] || { echo "missing image layout $layout; build it first" >&2; exit 1; }
  digest=$(python3 "$OCI" digest "$layout")
  repo="${IMAGE_REGISTRY}/currency-bot"
  if [[ "$PUSH_IMAGES" == 1 ]]; then
    out=$( cd "$ROOT" && "$(bazel)" run //solutions/currency-bot:currency-bot_push \
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
    echo "== image currency-bot: pushed ${repo}@${digest} (tag ${IMAGE_TAG})"
  elif [[ -n "${NODE_SSH:-}" ]]; then
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
    python3 "$OCI" pack "$layout" --ref "${repo}:${IMAGE_TAG}" --out "$tmp/currency-bot.tar" >/dev/null
    scp -q "$tmp/currency-bot.tar" "$NODE_SSH:/tmp/currency-bot-${IMAGE_TAG}.tar"
    # shellcheck disable=SC2029 # deliberately expanded locally into the remote command
    ssh "$NODE_SSH" "sudo -n k3s ctr -n k8s.io images import '/tmp/currency-bot-${IMAGE_TAG}.tar' >/dev/null && rm -f '/tmp/currency-bot-${IMAGE_TAG}.tar'"
    # shellcheck disable=SC2029
    ssh "$NODE_SSH" "sudo -n k3s ctr -n k8s.io images tag '${repo}:${IMAGE_TAG}' '${repo}@${digest}' >/dev/null"
    echo "== image currency-bot: imported into the node as ${repo}@${digest}"
  else
    echo "no delivery channel: set NODE_SSH (import) or PUSH_IMAGES=1 (push)" >&2
    exit 1
  fi
  IMAGE_REF="${repo}@${digest}"
fi

BUILD_ID="$(cd "$ROOT" && git rev-parse --short HEAD 2>/dev/null || echo nogit)-$(printf '%s' "$IMAGE_REF" | python3 -c 'import hashlib,sys; print(hashlib.sha256(sys.stdin.read().encode()).hexdigest()[:12])')"
echo "== build id: $BUILD_ID"

echo "== rendering manifests into $RENDER_DIR"
python3 "$ROOT/projects/home/kubernetes/scheduler/render.py" \
  --env "$PRIVATE_ENV" --out "$RENDER_DIR" \
  --set "BUILD_ID=$BUILD_ID" \
  --set "CURRENCY_BOT_IMAGE=$IMAGE_REF" \
  --set "CLOUDFLARED_IMAGE=$CLOUDFLARED_IMAGE" \
  --set "CURRENCY_WEBHOOK_URL=$WEBHOOK_URL" \
  "$DIR/manifests/currency-bot.yaml" \
  "$DIR/manifests/cloudflared.yaml" \
  "$DIR/manifests/network-policies.yaml" \
  "$DIR/manifests/servicemonitor.yaml"

echo "== namespace $NS"
if ! $KUBECTL get namespace "$NS" >/dev/null 2>&1; then
  echo "namespace is missing; applying manifests/namespace.yaml (admin action)" >&2
  $KUBECTL apply -f "$DIR/manifests/namespace.yaml"
fi

echo "== verifying ExternalSecrets (ESO-owned; CI never applies them)"
for es in currency-bot cloudflared-tunnel; do
  if ! $KUBECTL -n "$NS" get externalsecret "$es" >/dev/null 2>&1; then
    echo "missing ExternalSecret $NS/$es; apply it with" >&2
    echo "  projects/home/kubernetes/external-secrets/deploy.sh" >&2
    exit 1
  fi
  $KUBECTL -n "$NS" wait --for=condition=Ready "externalsecret/$es" --timeout=120s
done
for secret in currency-bot cloudflared-tunnel; do
  if ! $KUBECTL -n "$NS" get secret "$secret" >/dev/null 2>&1; then
    echo "missing secret $NS/$secret; apply the ExternalSecrets with" >&2
    echo "  projects/home/kubernetes/external-secrets/deploy.sh" >&2
    exit 1
  fi
done

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

echo "== applying currency-bot"
record_before currency-bot
converge_deployment currency-bot "$RENDER_DIR/currency-bot.yaml"
mark_applied currency-bot
$KUBECTL -n "$NS" rollout status deployment/currency-bot --timeout=180s

echo "== applying cloudflared"
record_before cloudflared
converge_deployment cloudflared "$RENDER_DIR/cloudflared.yaml"
mark_applied cloudflared
$KUBECTL -n "$NS" rollout status deployment/cloudflared --timeout=180s

echo "currency-bot is deployed in namespace $NS"
