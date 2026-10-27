#!/usr/bin/env bash
# Read-only checks for the post-Proxmox Tailscale Operator route.
# Run inside the K3s VM (or with a working kubeconfig + tailscale CLI):
#   TAILNET_HOST=pl-home.example-tailnet.ts.net ./verify-tailnet.sh
set -euo pipefail
: "${TAILNET_HOST:?Set TAILNET_HOST to the private tailnet hostname}"
: "${KUBECONFIG:?Set KUBECONFIG}"

fail() { printf 'FAIL  %s\n' "$*" >&2; exit 1; }
pass() { printf 'PASS  %s\n' "$*"; }

ingress="$(kubectl -n monitoring get ingress home -o jsonpath='{.metadata.name}' 2>/dev/null || true)"
[[ "$ingress" == "home" ]] || fail "Ingress monitoring/home not found"
pass "Operator Ingress monitoring/home exists"

# DNS must point at the tagged operator proxy, not the old host Serve.
if command -v tailscale >/dev/null 2>&1; then
  python3 - "$TAILNET_HOST" <<'PY' || fail "tailnet DNS/tag check"
import json, socket, subprocess, sys
host = sys.argv[1].rstrip('.')
status = json.loads(subprocess.check_output(['tailscale', 'status', '--json']))
peer = next((p for p in status.get('Peer', {}).values()
             if p.get('DNSName', '').rstrip('.') == host), None)
assert peer, f'{host}: no tailnet peer'
assert 'tag:k8s' in peer.get('Tags', []), f'{host}: tags are {peer.get("Tags")}'
assert socket.gethostbyname(host) in peer.get('TailscaleIPs', []), f'{host}: DNS mismatch'
print(f'PASS  {host} -> operator proxy tag:k8s')
PY
else
  printf 'WARN  tailscale CLI missing; DNS/tag check skipped\n'
fi

code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 20 "https://$TAILNET_HOST/" || true)"
[[ "$code" == "200" ]] || fail "HAOS route returned HTTP $code"
pass "HAOS route HTTP 200"

code="$(curl -sS -o /dev/null -w '%{http_code}' --max-time 20 "https://$TAILNET_HOST/grafana/api/health" || true)"
[[ "$code" == "200" ]] || fail "Grafana route returned HTTP $code"
pass "Grafana route HTTP 200"

printf 'Tailscale Operator route: PASS\n'
