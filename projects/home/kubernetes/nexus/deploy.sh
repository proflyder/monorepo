#!/usr/bin/env bash
# Deploy the Nexus artifact cache on the home K3s node.
#
# Admin action from the operator's Mac (like OpenBao/Vaultwarden): the
# namespace, the StorageClass-backed volume, the Traefik routes and the DNS
# record are cluster-wide or live in another namespace, so CI never applies
# this directory by itself.
#
# Private inputs — `mise run admin:nexus` passes them from OpenBao (fnox):
#   KUBECONFIG                       admin kubeconfig (required unless --render-only)
#   secrets/home/ingress.env         HOME_DOMAIN, LAN_CIDR, NODE_IP
#   secrets/home/vpn/cloudflare.env  CLOUDFLARE_API_TOKEN (internal A record)
# OpenBao (before the first run):
#   secret/k8s/nexus/nexus-secrets   admin_password, ci_reader_password
#
# Options:
#   --skip-dns          do not touch the internal Cloudflare A record
#   --render-only DIR   render the manifests into DIR and exit (tests)
#
# Idempotent: re-applying the same objects changes nothing; the provisioning
# Job is recreated so the REST state follows the script on every run.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/projects/home/kubernetes/versions.sh"

NS=nexus
DIR="$ROOT/projects/home/kubernetes/nexus"
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
CLOUDFLARE_API_TOKEN_FILE="${CLOUDFLARE_API_TOKEN_FILE:-$ROOT/secrets/home/vpn/cloudflare.env}"

# The public name a GitHub-hosted runner uses. It is deliberately separate from
# the internal one: the internal A record points at the LAN address for the UI
# and admin API, while the public name is a Cloudflare Tunnel hostname created
# by the owner (see README, "Internet exposure"). Override with
# NEXUS_PUBLIC_HOST when the tunnel host differs.
NEXUS_HOST="nexus.$HOME_DOMAIN"
NEXUS_PUBLIC_HOST="${NEXUS_PUBLIC_HOST:-ci-nexus.$HOME_DOMAIN}"
NEXUS_IMAGE="docker.io/sonatype/nexus3:${NEXUS_IMAGE_VERSION}@${NEXUS_IMAGE_DIGEST}"
NEXUS_PROVISION_IMAGE="docker.io/curlimages/curl:${NEXUS_PROVISION_IMAGE_VERSION}@${NEXUS_PROVISION_IMAGE_DIGEST}"

umask 077
OUT="${RENDER_ONLY:-$(mktemp -d)}"
[[ -n "$RENDER_ONLY" ]] || trap 'rm -rf "$OUT"' EXIT
mkdir -p "$OUT"

render() {
  python3 - "$1" "$OUT/$(basename "$1")" <<'PY'
import os, re, sys
src, dst = sys.argv[1], sys.argv[2]
keys = ("NEXUS_HOST", "NEXUS_PUBLIC_HOST", "NEXUS_IMAGE", "NEXUS_PROVISION_IMAGE",
        "NODE_LAN_IP", "LAN_CIDR")
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

export NEXUS_HOST NEXUS_PUBLIC_HOST NEXUS_IMAGE NEXUS_PROVISION_IMAGE LAN_CIDR
export NODE_LAN_IP="$NODE_IP"

for manifest in namespace external-secrets network-policies nexus provisioning ingressroute; do
  render "$DIR/manifests/$manifest.yaml"
done
cp "$DIR/provision.sh" "$OUT/provision.sh"

if [[ -n "$RENDER_ONLY" ]]; then
  echo "rendered into $OUT"
  exit 0
fi

: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"

echo "== namespace and ExternalSecret"
$KUBECTL apply -f "$OUT/namespace.yaml"
$KUBECTL apply -f "$OUT/external-secrets.yaml"
$KUBECTL -n "$NS" wait --for=condition=Ready externalsecret/nexus-secrets --timeout=120s
for key in admin_password ci_reader_password; do
  value=$($KUBECTL -n "$NS" get secret nexus-secrets -o jsonpath="{.data.$key}")
  [[ -n "$value" ]] || {
    echo "Secret $NS/nexus-secrets has no $key; write secret/k8s/nexus/nexus-secrets" >&2
    exit 1
  }
  unset value
done

echo "== NetworkPolicies and workload"
$KUBECTL apply -f "$OUT/network-policies.yaml"
$KUBECTL apply -f "$OUT/nexus.yaml"
$KUBECTL -n "$NS" rollout status statefulset/nexus --timeout=900s

echo "== provisioning Job"
$KUBECTL -n "$NS" create configmap nexus-provision \
  --from-file=provision.sh="$OUT/provision.sh" --dry-run=client -o yaml |
  $KUBECTL apply -f -
$KUBECTL -n "$NS" delete job nexus-provision --ignore-not-found
$KUBECTL apply -f "$OUT/provisioning.yaml"
if ! $KUBECTL -n "$NS" wait --for=condition=complete job/nexus-provision --timeout=900s; then
  echo "provisioning Job failed; last log lines:" >&2
  $KUBECTL -n "$NS" logs job/nexus-provision --tail=60 >&2 || true
  exit 1
fi

if [[ "$SKIP_DNS" == 1 ]]; then
  echo "skipping the internal Cloudflare A record (--skip-dns)"
else
  [[ -r "$CLOUDFLARE_API_TOKEN_FILE" ]] || {
    echo "Cloudflare token file not readable: $CLOUDFLARE_API_TOKEN_FILE" >&2
    exit 1
  }
  python3 "$ROOT/projects/home/kubernetes/cloudflare_dns.py" \
    "$CLOUDFLARE_API_TOKEN_FILE" "$NEXUS_HOST" "$NODE_IP"
fi

echo "== HTTPS routes"
$KUBECTL apply -f "$OUT/ingressroute.yaml"

echo "== smoke: status through the API-server service proxy"
$KUBECTL get --raw "/api/v1/namespaces/$NS/services/http:nexus:8081/proxy/service/rest/v1/status" >/dev/null
echo "Nexus is deployed."
echo "  internal UI/admin: https://$NEXUS_HOST/"
echo "  public CI cache:   https://$NEXUS_PUBLIC_HOST/repository/"
