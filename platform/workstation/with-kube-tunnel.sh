#!/usr/bin/env bash
# Run a command with the K3s API reachable at 127.0.0.1:16443, where the admin
# kubeconfig points. The API (6443) is firewalled from the LAN, so an SSH
# tunnel through the node is opened for the command and closed afterwards. An
# already listening 16443 (a tunnel you keep open) is reused and left alone.
#
#   with-kube-tunnel.sh <command> [args...]
set -euo pipefail
port="${KUBE_TUNNEL_PORT:-16443}"
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
