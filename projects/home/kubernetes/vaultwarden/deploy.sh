#!/usr/bin/env bash
# Deploy Vaultwarden on the home K3s node as https://passwords.<home-domain>/
# (LAN/VPN only, behind Traefik and the home allow-list).
#
# Admin action from the operator's Mac, like OpenBao: the namespace is
# cluster-scoped and the route lives in the `traefik` namespace.
#
# Private inputs (git-ignored; restore with openbao/materialize.py if absent):
#   KUBECONFIG                       admin kubeconfig (required unless --render-only)
#   secrets/home/ingress.env         HOME_DOMAIN, LAN_CIDR, NODE_IP
#   secrets/home/vpn/cloudflare.env  CLOUDFLARE_API_TOKEN (DNS record)
# OpenBao (before the first run):
#   secret/k8s/vaultwarden/vaultwarden-admin  ADMIN_TOKEN (Argon2id PHC hash)
#
# Options:
#   --skip-dns          do not touch the Cloudflare A record
#   --render-only DIR   render the manifests into DIR and exit (tests)
#
# Idempotent: re-running re-applies the same objects; the pod rolls only when
# the image or the pod template changes.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/projects/home/kubernetes/versions.sh"

NS=vaultwarden
DIR="$ROOT/projects/home/kubernetes/vaultwarden"
KUBECTL=${KUBECTL:-kubectl}
SKIP_DNS=0
RENDER_ONLY=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --skip-dns) SKIP_DNS=1 ;;
    --render-only) RENDER_ONLY=${2:?--render-only needs a directory}; shift ;;
    -h|--help) sed -n '2,/^set -euo pipefail/p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

PRIVATE_ENV="${INGRESS_PRIVATE_ENV:-$ROOT/secrets/home/ingress.env}"
if [[ -f "$PRIVATE_ENV" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$PRIVATE_ENV"
  set +a
fi
: "${HOME_DOMAIN:?Set HOME_DOMAIN (private ingress suffix) or provide $PRIVATE_ENV}"
: "${LAN_CIDR:?Set LAN_CIDR}"
: "${NODE_IP:?Set NODE_IP (K3s node LAN address)}"
CLOUDFLARE_API_TOKEN_FILE="${VAULTWARDEN_CLOUDFLARE_ENV:-$ROOT/secrets/home/vpn/cloudflare.env}"

PASSWORDS_HOST="passwords.$HOME_DOMAIN"
VAULTWARDEN_IMAGE="docker.io/vaultwarden/server:${VAULTWARDEN_IMAGE_VERSION}@${VAULTWARDEN_IMAGE_DIGEST}"

umask 077
OUT="${RENDER_ONLY:-$(mktemp -d)}"
[[ -n "$RENDER_ONLY" ]] || trap 'rm -rf "$OUT"' EXIT
mkdir -p "$OUT"

# Substitute exactly the known keys and refuse any leftover placeholder.
render() {
  python3 - "$1" "$OUT/$(basename "$1")" <<'PY'
import os, re, sys
src, dst = sys.argv[1], sys.argv[2]
keys = ("PASSWORDS_HOST", "VAULTWARDEN_IMAGE", "NODE_LAN_IP", "LAN_CIDR")
text = open(src, encoding="utf-8").read()
for key in keys:
    text = text.replace("${%s}" % key, os.environ[key])
body = "\n".join(l for l in text.splitlines() if not l.lstrip().startswith("#"))
left = re.findall(r"\$\{[A-Za-z_]+\}", body)
if left:
    sys.exit("unrendered placeholders in %s: %s" % (src, sorted(set(left))))
open(dst, "w", encoding="utf-8").write(text)
PY
}

export PASSWORDS_HOST VAULTWARDEN_IMAGE LAN_CIDR
export NODE_LAN_IP="$NODE_IP"
for manifest in namespace external-secrets network-policies vaultwarden ingressroute; do
  render "$DIR/manifests/$manifest.yaml"
done

if [[ -n "$RENDER_ONLY" ]]; then
  echo "rendered into $OUT"
  exit 0
fi

: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"

echo "== namespace and ExternalSecret"
$KUBECTL apply -f "$OUT/namespace.yaml"
$KUBECTL apply -f "$OUT/external-secrets.yaml"
$KUBECTL -n "$NS" wait --for=condition=Ready externalsecret/vaultwarden-admin --timeout=120s
admin_token=$($KUBECTL -n "$NS" get secret vaultwarden-admin -o jsonpath='{.data.ADMIN_TOKEN}')
[[ -n "$admin_token" ]] || {
  echo "Secret $NS/vaultwarden-admin has no ADMIN_TOKEN; write secret/k8s/vaultwarden/vaultwarden-admin" >&2
  exit 1
}
unset admin_token

echo "== NetworkPolicies and workload"
$KUBECTL apply -f "$OUT/network-policies.yaml"
$KUBECTL apply -f "$OUT/vaultwarden.yaml"
$KUBECTL -n "$NS" rollout status deployment/vaultwarden --timeout=300s

if [[ "$SKIP_DNS" == 1 ]]; then
  echo "skipping the Cloudflare A record (--skip-dns)"
else
  [[ -r "$CLOUDFLARE_API_TOKEN_FILE" ]] || {
    echo "Cloudflare token file not readable: $CLOUDFLARE_API_TOKEN_FILE" >&2
    exit 1
  }
  python3 "$ROOT/projects/home/kubernetes/cloudflare_dns.py" \
    "$CLOUDFLARE_API_TOKEN_FILE" "$PASSWORDS_HOST" "$NODE_IP"
fi

echo "== HTTPS route"
$KUBECTL apply -f "$OUT/ingressroute.yaml"

echo "== smoke: /alive through the API-server service proxy"
$KUBECTL get --raw "/api/v1/namespaces/$NS/services/http:vaultwarden:8080/proxy/alive" >/dev/null
echo "Vaultwarden is deployed: https://$PASSWORDS_HOST/ (admin panel: /admin)"
