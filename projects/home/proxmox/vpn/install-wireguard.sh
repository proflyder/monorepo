#!/usr/bin/env bash
set -euo pipefail
umask 077

ADDRESS=
PORT=51820
IFACE=wg0
SYNC_URL=
PRIVATE_KEY_FILE=
SYNC_TOKEN_FILE=
WG_DIR=/etc/wireguard

usage() {
  cat <<'EOF'
Usage: install-wireguard.sh --address CIDR --sync-url URL [options]

Required (real values from secrets/NETWORK.md):
  --address CIDR          server address inside the pool, e.g. 10.77.2.1/24
  --sync-url URL          scheduler peer list, e.g.
                          https://alarms.home.example.com/v1/vpn/peers

Options:
  --port PORT             UDP listen port (default 51820, never 443)
  --iface NAME            interface name (default wg0)
  --private-key-file PATH server private key to install (removed afterwards
                          by the caller); kept if omitted and already present
  --sync-token-file PATH  bearer token for the peer list; kept if omitted and
                          already present

The firewall (UDP port, forwarding and NAT of the pool) is owned by
install-ocserv.sh --wg-cidr/--wg-port; this script never touches nftables or
ocserv. Peers are never edited by hand: wg-peer-sync pulls them from the
scheduler every 30 seconds and applies changes with `wg syncconf`, so live
sessions are not interrupted.
EOF
}

die() { echo "error: $*" >&2; exit 1; }

while (($#)); do
  case "$1" in
    --address) ADDRESS=$2; shift 2 ;;
    --port) PORT=$2; shift 2 ;;
    --iface) IFACE=$2; shift 2 ;;
    --sync-url) SYNC_URL=$2; shift 2 ;;
    --private-key-file) PRIVATE_KEY_FILE=$2; shift 2 ;;
    --sync-token-file) SYNC_TOKEN_FILE=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument: $1" ;;
  esac
done

[[ $(id -u) == 0 ]] || die "run inside the container as root"
[[ -n $ADDRESS && -n $SYNC_URL ]] || { usage >&2; die "--address and --sync-url are required"; }
[[ $ADDRESS =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+/[0-9]+$ ]] || die "--address must be IPv4 CIDR: $ADDRESS"
[[ $PORT =~ ^[0-9]+$ && $PORT -ge 1024 && $PORT -le 65535 && $PORT != 443 ]] ||
  die "--port must be 1024..65535 and not 443: $PORT"
[[ $IFACE =~ ^[a-z0-9]+$ ]] || die "--iface is invalid: $IFACE"
[[ $SYNC_URL == https://* ]] || die "--sync-url must be https"

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends ca-certificates curl jq wireguard-tools

probe=wgprobe$$
ip link add "$probe" type wireguard 2>/dev/null ||
  die "kernel WireGuard is unavailable: load the module on the Proxmox host (modules-load.d)"
ip link del "$probe"

install -d -m 700 "$WG_DIR"
if [[ -n $PRIVATE_KEY_FILE ]]; then
  key=$(tr -d '[:space:]' <"$PRIVATE_KEY_FILE")
  [[ $key =~ ^[A-Za-z0-9+/]{42}[AEIMQUYcgkosw048]=$ ]] || die "private key is not a WireGuard key"
  printf '%s\n' "$key" | install -m 600 /dev/stdin "$WG_DIR/server.key"
  unset key
fi
[[ -s $WG_DIR/server.key ]] || die "no server key: pass --private-key-file"

if [[ -n $SYNC_TOKEN_FILE ]]; then
  token=$(tr -d '[:space:]' <"$SYNC_TOKEN_FILE")
  [[ -n $token ]] || die "sync token is empty"
  printf 'Authorization: Bearer %s\n' "$token" | install -m 600 /dev/stdin "$WG_DIR/sync.header"
  unset token
fi
[[ -s $WG_DIR/sync.header ]] || die "no sync token: pass --sync-token-file"

cat >"$WG_DIR/sync.env" <<EOF
WG_IFACE=$IFACE
WG_ADDRESS=$ADDRESS
WG_PORT=$PORT
WG_SYNC_URL=$SYNC_URL
EOF
chmod 600 "$WG_DIR/sync.env"

script_dir=$(cd "$(dirname "$0")" && pwd)
install -m 755 "$script_dir/wg-peer-sync.sh" /usr/local/sbin/wg-peer-sync

cat >/etc/systemd/system/wg-peer-sync.service <<'EOF'
[Unit]
Description=Pull WireGuard peers from the home scheduler
After=network-online.target
Wants=network-online.target

[Service]
Type=oneshot
ExecStart=/usr/local/sbin/wg-peer-sync
EOF

cat >/etc/systemd/system/wg-peer-sync.timer <<'EOF'
[Unit]
Description=Pull WireGuard peers from the home scheduler every 30 seconds

[Timer]
OnBootSec=20s
OnUnitActiveSec=30s
AccuracySec=5s

[Install]
WantedBy=timers.target
EOF

systemctl daemon-reload
/usr/local/sbin/wg-peer-sync --render-only
systemctl enable "wg-quick@$IFACE"
if systemctl is-active --quiet "wg-quick@$IFACE"; then
  wg syncconf "$IFACE" <(wg-quick strip "$IFACE")
else
  systemctl start "wg-quick@$IFACE"
fi
systemctl enable --now wg-peer-sync.timer
systemctl start wg-peer-sync.service || echo "warning: first peer sync failed; the timer retries" >&2

echo "WireGuard $IFACE listening on udp/$PORT; server public key: $(wg pubkey <"$WG_DIR/server.key")"
