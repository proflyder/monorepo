#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
: "${KUBECONFIG:?Set private kubeconfig}"
: "${TAILSCALE_OAUTH_FILE:?Set private JSON with client_id/client_secret}"
kubectl create namespace tailscale --dry-run=client -o yaml | kubectl apply -f -
python3 - "$TAILSCALE_OAUTH_FILE" <<'PY' | kubectl apply -f -
import json,sys,base64
with open(sys.argv[1]) as f:
    credentials=json.load(f)
assert credentials['client_secret'].startswith('tskey-client-')
print(json.dumps({'apiVersion':'v1','kind':'Secret','metadata':{'name':'operator-oauth','namespace':'tailscale'},
                 'type':'Opaque','data':{k:base64.b64encode(credentials[k].encode()).decode() for k in ('client_id','client_secret')}}))
PY
helm upgrade --install tailscale-operator tailscale/tailscale-operator \
  --version 1.104.2 -n tailscale \
  -f "$ROOT/projects/home/kubernetes/tailscale/operator-values.yaml" --wait --timeout 5m
