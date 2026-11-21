#!/usr/bin/env bash
set -euo pipefail
umask 077

WG_DIR=${WG_DIR:-/etc/wireguard}
RENDER_ONLY=0
FROM_FILE=

while (($#)); do
  case "$1" in
    --render-only) RENDER_ONLY=1; shift ;;
    --from-file) FROM_FILE=$2; shift 2 ;;
    *) echo "usage: wg-peer-sync [--render-only] [--from-file JSON]" >&2; exit 2 ;;
  esac
done

source "$WG_DIR/sync.env"
CACHE=$WG_DIR/peers.json
CONF=$WG_DIR/$WG_IFACE.conf

valid_peers() {
  jq -e '
    (.peers | type == "array")
    and (.peers | length <= 254)
    and all(.peers[];
      (.publicKey | type == "string" and test("^[A-Za-z0-9+/]{42}[AEIMQUYcgkosw048]=$"))
      and (.allowedIp | type == "string" and test("^[0-9]{1,3}(\\.[0-9]{1,3}){3}/32$")))
    and ([.peers[].publicKey] | length == (unique | length))
    and ([.peers[].allowedIp] | length == (unique | length))
  ' "$1" >/dev/null
}

render() {
  local peers=$1
  printf '[Interface]\nAddress = %s\nListenPort = %s\nPrivateKey = %s\n' \
    "$WG_ADDRESS" "$WG_PORT" "$(tr -d '[:space:]' <"$WG_DIR/server.key")"
  if [[ -s $peers ]]; then
    jq -r '.peers[] | "\n[Peer]\nPublicKey = \(.publicKey)\nAllowedIPs = \(.allowedIp)"' "$peers"
  fi
}

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

if ((RENDER_ONLY == 0)); then
  if [[ -n $FROM_FILE ]]; then
    cp "$FROM_FILE" "$work/peers.json"
  else
    curl -fsS --max-time 15 --proto =https -H @"$WG_DIR/sync.header" \
      -o "$work/peers.json" "$WG_SYNC_URL"
  fi
  valid_peers "$work/peers.json" || { echo "peer list rejected: invalid JSON shape" >&2; exit 1; }
  jq -S '{peers: [.peers[] | {publicKey, allowedIp}] | sort_by(.allowedIp)}' \
    "$work/peers.json" >"$work/peers.sorted"
  install -m 600 "$work/peers.sorted" "$CACHE"
fi

render "$CACHE" >"$work/conf"
if [[ -f $CONF ]] && cmp -s "$work/conf" "$CONF"; then
  exit 0
fi
install -m 600 "$work/conf" "$CONF"
if ((RENDER_ONLY == 0)) && ip link show "$WG_IFACE" >/dev/null 2>&1; then
  wg syncconf "$WG_IFACE" <(wg-quick strip "$WG_IFACE")
  echo "peers applied: $(jq '.peers | length' "$CACHE")"
fi
