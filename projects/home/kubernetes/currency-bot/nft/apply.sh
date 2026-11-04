#!/usr/bin/env bash
# Install or refresh the currency-bot NodePort source filter on the K3s node.
#
# This is host configuration, not part of a CI deploy: the API deploy path
# never touches nftables. Run it as root on the node (or over SSH) once, and
# again when the LAN CIDR, node address or NodePort changes.
#
#   PRIVATE_ENV=secrets/home/currency-bot.env ./apply.sh
#   ./apply.sh --env /path/to/currency-bot.env
#
# Rendering needs the Go `home-deploy` binary on this node (an installed
# /usr/local/bin/home-deploy, or set HOME_DEPLOY); see platform/deploy.
#
# The rendered ruleset replaces only `table inet ricardo_currency_bot`; the
# Kubernetes, scheduler and management tables are untouched.
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../../../.." && pwd)"
# shellcheck source=platform/deploy/resolve.sh
source "$ROOT/platform/deploy/resolve.sh"
PRIVATE_ENV=${PRIVATE_ENV:-"$ROOT/secrets/home/currency-bot.env"}
NFT_BIN=${NFT_BIN:-/usr/sbin/nft}
TARGET=/etc/ricardo-currency-bot-nodeport.nft

while [[ $# -gt 0 ]]; do
  case "$1" in
    --env) PRIVATE_ENV=$2; shift ;;
    -h|--help) sed -n '2,/^set -euo pipefail/p' "$0"; exit 0 ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

[[ $(id -u) == 0 ]] || { echo "run as root (nft and $TARGET need root)" >&2; exit 1; }
[[ -f "$PRIVATE_ENV" ]] || { echo "missing private values file: $PRIVATE_ENV" >&2; exit 1; }

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
home_deploy render --unit scheduler --env "$PRIVATE_ENV" --out "$tmp" "$DIR/currency-bot-nodeport.nft"

"$NFT_BIN" -c -f "$tmp/currency-bot-nodeport.nft"
install -m 0644 "$tmp/currency-bot-nodeport.nft" "$TARGET"
if "$NFT_BIN" list table inet ricardo_currency_bot >/dev/null 2>&1; then
  { echo 'delete table inet ricardo_currency_bot'; cat "$TARGET"; } | "$NFT_BIN" -f -
else
  "$NFT_BIN" -f "$TARGET"
fi
"$NFT_BIN" list table inet ricardo_currency_bot >/dev/null
# Boot persistence: the unit reloads the rendered file after every reboot.
install -m 0644 "$DIR/ricardo-currency-bot-firewall.service" /etc/systemd/system/ricardo-currency-bot-firewall.service
systemctl daemon-reload
systemctl enable ricardo-currency-bot-firewall.service >/dev/null
echo "currency-bot NodePort filter installed from $TARGET (persistent via ricardo-currency-bot-firewall.service)"
