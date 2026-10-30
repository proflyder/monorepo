#!/usr/bin/env bash
# Deploy Traefik + cert-manager for the private home HTTPS suffix.
#
# Public inputs live here; private values come from an untracked environment
# file (default: secrets/home/ingress.env) or the caller's environment:
#   HOME_DOMAIN              private ingress suffix, e.g. home.example.com
#   LAN_CIDR                 LAN range allowed by the IP allow-list
#   CLOUDFLARE_API_TOKEN_FILE  file with CLOUDFLARE_API_TOKEN=<token>
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/projects/home/kubernetes/versions.sh"

: "${KUBECONFIG:?Set KUBECONFIG to a private admin kubeconfig}"
command -v helm >/dev/null || { echo "helm is required" >&2; exit 1; }
command -v kubectl >/dev/null || { echo "kubectl is required" >&2; exit 1; }

STAGING=0
for arg in "$@"; do
  case "$arg" in
    --staging) STAGING=1 ;;
    -h|--help) sed -n '2,10p' "$0"; exit 0 ;;
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

LAN_CIDR="${LAN_CIDR:-192.0.2.0/24}"
CLOUDFLARE_API_TOKEN_FILE="${CLOUDFLARE_API_TOKEN_FILE:-$ROOT/secrets/home/vpn/cloudflare.env}"
[[ -r "$CLOUDFLARE_API_TOKEN_FILE" ]] || {
  echo "Cloudflare token file not readable: $CLOUDFLARE_API_TOKEN_FILE" >&2
  exit 1
}

TRAEFIK_NS=traefik
CERT_MANAGER_NS=cert-manager
MANIFESTS="$ROOT/projects/home/kubernetes/ingress/manifests"
VALUES="$ROOT/projects/home/kubernetes/ingress/values"

HA_HOST="ha.$HOME_DOMAIN"
GRAFANA_HOST="grafana.$HOME_DOMAIN"
ALARMS_HOST="alarms.$HOME_DOMAIN"
CONTROL_HOST="control.$HOME_DOMAIN"
export HOME_DOMAIN LAN_CIDR HA_HOST GRAFANA_HOST ALARMS_HOST CONTROL_HOST

render() {
  python3 - "$1" <<'PY'
import os
import sys

text = open(sys.argv[1], encoding="utf-8").read()
for key in ("HOME_DOMAIN", "LAN_CIDR",
            "HA_HOST", "GRAFANA_HOST", "ALARMS_HOST", "CONTROL_HOST"):
    text = text.replace("${%s}" % key, os.environ[key])
if "${" in text:
    sys.exit("unrendered placeholder in %s" % sys.argv[1])
sys.stdout.write(text)
PY
}

apply_cloudflare_secret() {
  python3 - "$CLOUDFLARE_API_TOKEN_FILE" "$CERT_MANAGER_NS" <<'PY' | kubectl apply -f -
import base64
import json
import sys

path, namespace = sys.argv[1], sys.argv[2]
token = None
with open(path, encoding="utf-8") as handle:
    for line in handle:
        line = line.strip()
        if line.startswith("CLOUDFLARE_API_TOKEN="):
            token = line.split("=", 1)[1].strip().strip('"').strip("'")
            break
if not token:
    sys.exit("CLOUDFLARE_API_TOKEN is missing in %s" % path)
print(json.dumps({
    "apiVersion": "v1",
    "kind": "Secret",
    "metadata": {"name": "cloudflare-api-token", "namespace": namespace},
    "type": "Opaque",
    "data": {"api-token": base64.b64encode(token.encode()).decode()},
}))
PY
}

helm repo add traefik https://traefik.github.io/charts --force-update >/dev/null
helm repo add jetstack https://charts.jetstack.io --force-update >/dev/null
helm repo update >/dev/null

kubectl create namespace "$CERT_MANAGER_NS" --dry-run=client -o yaml | kubectl apply -f -
helm upgrade --install cert-manager jetstack/cert-manager \
  --version "$CERT_MANAGER_CHART_VERSION" -n "$CERT_MANAGER_NS" \
  -f "$VALUES/cert-manager.yaml" --wait --timeout 8m
kubectl wait --for=condition=Established --timeout=120s \
  crd/clusterissuers.cert-manager.io crd/certificates.cert-manager.io

kubectl create namespace "$TRAEFIK_NS" --dry-run=client -o yaml | kubectl apply -f -
apply_cloudflare_secret

render "$MANIFESTS/issuers.yaml" | kubectl apply -f -

if [[ "$STAGING" == 1 ]]; then
  echo "Issuing a staging certificate to validate DNS-01..."
  render "$MANIFESTS/certificate-staging.yaml" | kubectl apply -f -
  kubectl -n "$TRAEFIK_NS" wait --for=condition=Ready certificate/home-wildcard-staging \
    --timeout=600s
fi

render "$MANIFESTS/certificate.yaml" | kubectl apply -f -

helm upgrade --install traefik traefik/traefik \
  --version "$TRAEFIK_CHART_VERSION" -n "$TRAEFIK_NS" \
  -f "$VALUES/traefik.yaml" --wait --timeout 8m

render "$MANIFESTS/middleware.yaml" | kubectl apply -f -
render "$MANIFESTS/routes.yaml" | kubectl apply -f -

kubectl -n "$TRAEFIK_NS" wait --for=condition=Ready certificate/home-wildcard \
  --timeout=600s

kubectl -n "$TRAEFIK_NS" get certificate home-wildcard \
  -o jsonpath='{.status.conditions[?(@.type=="Ready")].message}{"\n"}'
echo "Ingress deployed for *.$HOME_DOMAIN"
