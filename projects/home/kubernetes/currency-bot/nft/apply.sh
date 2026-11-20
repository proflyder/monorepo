#!/usr/bin/env bash
set -euo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$DIR/../../../../.." && pwd)"
# shellcheck source=platform/deploy/resolve.sh
source "$ROOT/platform/deploy/resolve.sh"
PRIVATE_ENV=${PRIVATE_ENV:-"$ROOT/secrets/home/currency-bot.env"}
NFT_BIN=${NFT_BIN:-/usr/sbin/nft}
TARGET=/etc/home-currency-bot-nodeport.nft

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
if "$NFT_BIN" list table inet home_currency_bot >/dev/null 2>&1; then
  { echo 'delete table inet home_currency_bot'; cat "$TARGET"; } | "$NFT_BIN" -f -
else
  "$NFT_BIN" -f "$TARGET"
fi
"$NFT_BIN" list table inet home_currency_bot >/dev/null
install -m 0644 "$DIR/home-currency-bot-firewall.service" /etc/systemd/system/home-currency-bot-firewall.service
systemctl daemon-reload
systemctl enable home-currency-bot-firewall.service >/dev/null
echo "currency-bot NodePort filter installed from $TARGET (persistent via home-currency-bot-firewall.service)"
