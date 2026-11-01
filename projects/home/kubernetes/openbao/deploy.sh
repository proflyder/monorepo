#!/usr/bin/env bash
# Deploy OpenBao (single node, raft, static auto-unseal, UI) on the home K3s
# node and publish it as https://vault.<home-domain>/ behind Traefik.
#
# Public inputs live here; private values come from untracked files:
#   KUBECONFIG                 admin kubeconfig (required)
#   secrets/home/ingress.env   HOME_DOMAIN, NODE_IP (same file as ingress/)
#   secrets/home/vpn/cloudflare.env  CLOUDFLARE_API_TOKEN (DNS record)
#   secrets/openbao/seal.key   created on first run (32 random bytes)
#
# Optional environment:
#   OPENBAO_NODE_SSH  full ssh command for the K3s node, used to create
#                     /srv/k3s/volumes/openbao{,-backups}
#   OPENBAO_NODE_NAME node name for PV nodeAffinity (default: the only node)
#   OPENBAO_SKIP_DNS=1  do not touch the Cloudflare A record
#
# Idempotent: re-running upgrades the release, refreshes the seal Secret and
# the DNS record, and leaves an initialised/unsealed server untouched.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/projects/home/kubernetes/versions.sh"

: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"
command -v helm >/dev/null || { echo "helm is required" >&2; exit 1; }
command -v kubectl >/dev/null || { echo "kubectl is required" >&2; exit 1; }

SKIP_DNS=0
for arg in "$@"; do
  case "$arg" in
    --skip-dns) SKIP_DNS=1 ;;
    -h|--help) sed -n '2,26p' "$0"; exit 0 ;;
    *) echo "unknown argument: $arg" >&2; exit 2 ;;
  esac
done

PRIVATE_ENV="${INGRESS_PRIVATE_ENV:-$ROOT/secrets/home/ingress.env}"
if [[ -f "$PRIVATE_ENV" ]]; then
  set -a
  # shellcheck disable=SC1090
  source "$PRIVATE_ENV"
  set +a
fi

: "${HOME_DOMAIN:?Set HOME_DOMAIN (private ingress suffix)}"
VAULT_HOST="vault.$HOME_DOMAIN"
NODE_IP="${NODE_IP:-192.0.2.27}"
CLOUDFLARE_API_TOKEN_FILE="${CLOUDFLARE_API_TOKEN_FILE:-$ROOT/secrets/home/vpn/cloudflare.env}"

OPENBAO_NS=openbao
SECRETS_DIR="$ROOT/secrets/openbao"
MANIFESTS="$ROOT/projects/home/kubernetes/openbao/manifests"
VALUES="$ROOT/projects/home/kubernetes/openbao/values"
IMAGE_TAG="${OPENBAO_IMAGE_VERSION}@${OPENBAO_IMAGE_DIGEST}"

umask 077
install -d -m 700 "$SECRETS_DIR"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
chmod 700 "$TMP"

# ----- seal key and namespace ---------------------------------------------
if [[ ! -s "$SECRETS_DIR/seal.key" ]]; then
  openssl rand -out "$SECRETS_DIR/seal.key" 32
  chmod 600 "$SECRETS_DIR/seal.key"
  echo "generated $SECRETS_DIR/seal.key"
fi
[[ $(wc -c <"$SECRETS_DIR/seal.key" | tr -d ' ') == 32 ]] || {
  echo "seal key must be exactly 32 bytes: $SECRETS_DIR/seal.key" >&2
  exit 1
}

kubectl create namespace "$OPENBAO_NS" --dry-run=client -o yaml | kubectl apply -f -
kubectl -n "$OPENBAO_NS" create secret generic openbao-seal \
  --from-file=unseal.key="$SECRETS_DIR/seal.key" \
  --dry-run=client -o yaml | kubectl apply -f -

# ----- local volume directories and PVs -----------------------------------
NODE_NAME="${OPENBAO_NODE_NAME:-$(kubectl get nodes -o jsonpath='{.items[0].metadata.name}')}"
if [[ -n "${OPENBAO_NODE_SSH:-}" ]]; then
  $OPENBAO_NODE_SSH 'sudo install -d -m 700 /srv/k3s/volumes/openbao /srv/k3s/volumes/openbao-backups'
else
  echo "note: set OPENBAO_NODE_SSH to create /srv/k3s/volumes/openbao* on the node on first install" >&2
fi
sed "s|__OPENBAO_NODE_NAME__|$NODE_NAME|g" "$MANIFESTS/storage.yaml" | kubectl apply -f -

# ----- helm release --------------------------------------------------------
sed "s|__OPENBAO_IMAGE_TAG__|$IMAGE_TAG|g" "$VALUES/openbao.yaml" > "$TMP/values.yaml"
chmod 600 "$TMP/values.yaml"

# StatefulSet volumeClaimTemplates are immutable. A release that still has the
# old audit volume is removed with --cascade=orphan (the pod and data PVC stay)
# so helm can recreate it without the audit claim.
if kubectl -n "$OPENBAO_NS" get statefulset openbao \
    -o jsonpath='{.spec.volumeClaimTemplates[*].metadata.name}' 2>/dev/null \
    | tr ' ' '\n' | grep -qx audit; then
  kubectl -n "$OPENBAO_NS" delete statefulset openbao --cascade=orphan
fi

helm repo add openbao https://openbao.github.io/openbao-helm --force-update >/dev/null
helm repo update openbao >/dev/null
# --wait would block on the readiness probe: a fresh server is sealed until
# bootstrap.sh initialises it.
helm upgrade --install openbao openbao/openbao \
  --version "$OPENBAO_CHART_VERSION" -n "$OPENBAO_NS" \
  -f "$TMP/values.yaml"

kubectl -n "$OPENBAO_NS" wait --for=jsonpath='{.status.phase}'=Running \
  pod/openbao-0 --timeout=180s

# ----- DNS A record (Cloudflare, DNS only) --------------------------------
if [[ "$SKIP_DNS" == 1 ]]; then
  echo "skipping the Cloudflare A record (--skip-dns)"
else
  [[ -r "$CLOUDFLARE_API_TOKEN_FILE" ]] || {
    echo "Cloudflare token file not readable: $CLOUDFLARE_API_TOKEN_FILE" >&2
    exit 1
  }
  python3 - "$CLOUDFLARE_API_TOKEN_FILE" "$VAULT_HOST" "$NODE_IP" <<'PY'
import json
import sys
import urllib.error
import urllib.request

token_file, name, address = sys.argv[1], sys.argv[2], sys.argv[3]
token = None
with open(token_file, encoding="utf-8") as handle:
    for line in handle:
        line = line.strip()
        if line.startswith("CLOUDFLARE_API_TOKEN="):
            token = line.split("=", 1)[1].strip().strip('"').strip("'")
            break
if not token:
    sys.exit("CLOUDFLARE_API_TOKEN is missing in %s" % token_file)

BASE = "https://api.cloudflare.com/client/v4"


def call(method, path, body=None):
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(
        BASE + path, data=data, method=method,
        headers={"Authorization": "Bearer " + token,
                 "Content-Type": "application/json"})
    try:
        with urllib.request.urlopen(request, timeout=20) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        sys.exit("Cloudflare API %s %s failed: %s" % (
            method, path, error.read().decode(errors="replace")))


zones = call("GET", "/zones?per_page=50")["result"]
matches = [zone for zone in zones
           if name == zone["name"] or name.endswith("." + zone["name"])]
if not matches:
    sys.exit("no Cloudflare zone contains %s" % name)
zone = max(matches, key=lambda entry: len(entry["name"]))

records = call("GET", "/zones/%s/dns_records?type=A&name=%s" % (zone["id"], name))["result"]
body = {"type": "A", "name": name, "content": address, "ttl": 1, "proxied": False}
if records:
    record = records[0]
    if record["content"] == address and record.get("proxied") is False:
        print("DNS A %s -> %s already set (DNS only)" % (name, address))
    else:
        call("PUT", "/zones/%s/dns_records/%s" % (zone["id"], record["id"]), body)
        print("DNS A %s -> %s updated (DNS only)" % (name, address))
else:
    call("POST", "/zones/%s/dns_records" % zone["id"], body)
    print("DNS A %s -> %s created (DNS only)" % (name, address))
PY
fi

# ----- HTTPS route ---------------------------------------------------------
sed "s|\${VAULT_HOST}|$VAULT_HOST|g" "$MANIFESTS/ingressroute.yaml" | kubectl apply -f -
kubectl -n traefik get ingressroute openbao >/dev/null

# ----- backup job and probe manifests -------------------------------------
sed "s|__OPENBAO_IMAGE_TAG__|$IMAGE_TAG|g" "$MANIFESTS/backup.yaml" | kubectl apply -f -

status=$(kubectl -n "$OPENBAO_NS" exec openbao-0 -- \
  /bin/sh -ec 'BAO_ADDR=http://127.0.0.1:8200 bao status -format=json' 2>/dev/null || true)
if [[ "$status" == *'"initialized": true'* ]]; then
  echo "OpenBao is already initialised."
  if [[ "$status" == *'"sealed": true'* ]]; then
    echo "WARNING: server is sealed; restart the pod so the static seal unseals it." >&2
  fi
else
  echo "OpenBao is not initialised yet: run ./projects/home/kubernetes/openbao/bootstrap.sh"
fi

echo "OpenBao release deployed for https://$VAULT_HOST/"
echo "probe manifest (apply and run): $MANIFESTS/smoke-kubernetes.yaml"
