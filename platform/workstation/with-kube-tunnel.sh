#!/usr/bin/env bash
set -euo pipefail
port="${KUBE_TUNNEL_PORT:-16443}"
expected="https://127.0.0.1:${port}"
if [[ -z "${KUBECONFIG:-}" ]]; then
  echo "with-kube-tunnel: KUBECONFIG is required; use \`mise run kubectl\`" >&2
  exit 1
fi
if [[ ! -f "$KUBECONFIG" ]]; then
  echo "with-kube-tunnel: KUBECONFIG file does not exist: $KUBECONFIG" >&2
  exit 1
fi
server="$(kubectl config view --minify --kubeconfig="$KUBECONFIG" -o jsonpath='{.clusters[0].cluster.server}' 2>/dev/null || true)"
if [[ "$server" != "$expected" ]]; then
  echo "with-kube-tunnel: refusing kubeconfig server '${server:-<empty>}', expected $expected" >&2
  exit 1
fi
config="${SSH_CONFIG:-$HOME/.ssh/home-lab/config}"
listening() { nc -z 127.0.0.1 "$port" >/dev/null 2>&1; }

if listening; then
  exec "$@"
fi
[[ -f "$config" ]] || { echo "no $config: run \`mise run secrets:pull\`" >&2; exit 1; }
socket="$(mktemp -u "${TMPDIR:-/tmp}/kube-tunnel.XXXXXX")"
ssh -F "$config" -f -N -M -S "$socket" -o ExitOnForwardFailure=yes \
  -L "127.0.0.1:$port:127.0.0.1:6443" k3s
trap 'ssh -F "$config" -S "$socket" -O exit k3s >/dev/null 2>&1 || true' EXIT
for _ in 1 2 3 4 5 6 7 8 9 10; do listening && break; sleep 0.3; done
"$@"
