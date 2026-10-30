#!/usr/bin/env bash
# Install and configure ocserv inside the VPN container.
#
# Run as root inside the CT (create-lxc.sh creates the container). The script
# is idempotent: existing users, OTP secrets and a generated certificate are
# preserved. Public examples use RFC 5737 addresses; real values are passed in.
set -euo pipefail
umask 077

LAN_CIDR=
DNS=
SERVER_NAME=vpn.example.com
SERVER_IP=
IFACE=eth0
VPN_NETWORK=10.77.0.0
VPN_NETMASK=255.255.255.0
VPN_CIDR=10.77.0.0/24

usage() {
  cat <<'EOF'
Usage: install-ocserv.sh --lan-cidr CIDR --dns ADDRESS --server-ip ADDRESS [options]

Required (real values from secrets/NETWORK.md):
  --lan-cidr CIDR       home LAN, e.g. 192.0.2.0/24
  --dns ADDRESS         DNS pushed to clients, e.g. 192.0.2.1
  --server-ip ADDRESS   container IPv4 address (certificate SAN)

Options:
  --server-name NAME    public certificate name (default vpn.example.com)
  --iface NAME          uplink interface inside the CT (default eth0)
  --vpn-network ADDR    VPN pool network (default 10.77.0.0)
  --vpn-netmask MASK    VPN pool netmask (default 255.255.255.0)
  --vpn-cidr CIDR       VPN pool in CIDR form (default 10.77.0.0/24)
EOF
}

die() { echo "error: $*" >&2; exit 1; }

while (($#)); do
  case "$1" in
    --lan-cidr) LAN_CIDR=$2; shift 2 ;;
    --dns) DNS=$2; shift 2 ;;
    --server-name) SERVER_NAME=$2; shift 2 ;;
    --server-ip) SERVER_IP=$2; shift 2 ;;
    --iface) IFACE=$2; shift 2 ;;
    --vpn-network) VPN_NETWORK=$2; shift 2 ;;
    --vpn-netmask) VPN_NETMASK=$2; shift 2 ;;
    --vpn-cidr) VPN_CIDR=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument: $1" ;;
  esac
done

[[ $(id -u) == 0 ]] || die "run inside the container as root"
[[ -n $LAN_CIDR && -n $DNS && -n $SERVER_IP ]] || {
  usage >&2
  die "--lan-cidr, --dns and --server-ip are required"
}

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates gnutls-bin nftables ocserv openssh-server

install -d -m 755 /etc/ocserv /etc/ocserv/ssl /etc/ocserv/config-per-group \
  /etc/ocserv/defaults

# ----- TLS certificate (self-signed for stage 1) ---------------------------
if [[ ! -s /etc/ocserv/ssl/server-cert.pem || ! -s /etc/ocserv/ssl/server-key.pem ]]; then
  tmp=$(mktemp -d)
  trap 'rm -rf "$tmp"' EXIT
  chmod 700 "$tmp"
  certtool --generate-privkey --outfile "$tmp/server-key.pem"
  cat >"$tmp/server.tmpl" <<EOF
cn = "$SERVER_NAME"
organization = "home"
expiration_days = 825
signing_key
encryption_key
tls_www_server
dns_name = "$SERVER_NAME"
ip_address = "$SERVER_IP"
EOF
  certtool --generate-self-signed \
    --load-privkey "$tmp/server-key.pem" \
    --template "$tmp/server.tmpl" \
    --outfile "$tmp/server-cert.pem"
  install -m 600 "$tmp/server-key.pem" /etc/ocserv/ssl/server-key.pem
  install -m 644 "$tmp/server-cert.pem" /etc/ocserv/ssl/server-cert.pem
fi

# ----- ocserv configuration ------------------------------------------------
cat >/etc/ocserv/ocserv.conf <<EOF
# Managed by projects/home/proxmox/vpn/install-ocserv.sh. Do not edit by hand.

# Password + TOTP. Passwords live in ocpasswd, TOTP keys in users.oath.
auth = "plain[passwd=/etc/ocserv/ocpasswd,otp=/etc/ocserv/users.oath]"

tcp-port = 443
udp-port = 443
run-as-user = ocserv
run-as-group = ocserv
socket-file = /var/run/ocserv-socket
pid-file = /var/run/ocserv.pid

server-cert = /etc/ocserv/ssl/server-cert.pem
server-key = /etc/ocserv/ssl/server-key.pem

isolate-workers = true
max-clients = 16
max-same-clients = 2
rate-limit-ms = 200
keepalive = 32400
dpd = 90
mobile-dpd = 1800
switch-to-tcp-timeout = 25
try-mtu-discovery = true
mtu = 1400

# TLS 1.2+ only; no SSLv3/TLS 1.0/TLS 1.1, no compression.
tls-priorities = "NORMAL:%SERVER_PRECEDENCE:%COMPAT:-VERS-SSL3.0:-VERS-TLS1.0:-VERS-TLS1.1"
compression = false
cisco-client-compat = true

# Ban brute force attempts (ocserv point system). Only wrong passwords score:
# mobile clients reconnect in bursts and must not ban themselves.
min-reauth-time = 300
auth-timeout = 120
max-ban-score = 50
ban-reset-time = 1200
ban-points-wrong-password = 10
ban-points-connection = 0

cookie-timeout = 300
deny-roaming = false
rekey-time = 172800
rekey-method = ssl

predictable-ips = true
use-occtl = true
log-level = 2

# Two profiles selectable at login. "lan" is the default split tunnel:
# only the home LAN is routed. "full" sends all traffic through home.
select-group = lan
select-group = full[Full tunnel]
default-select-group = lan
config-per-group = /etc/ocserv/config-per-group/
default-group-config = /etc/ocserv/defaults/group.conf

ipv4-network = $VPN_NETWORK
ipv4-netmask = $VPN_NETMASK
device = vpns
EOF

cat >/etc/ocserv/config-per-group/lan <<EOF
# Split tunnel: home LAN only.
route = $LAN_CIDR
dns = $DNS
tunnel-all-dns = false
EOF

cat >/etc/ocserv/config-per-group/full <<EOF
# Full tunnel: all client traffic exits through home.
route = default
dns = $DNS
tunnel-all-dns = true
EOF

cp /etc/ocserv/config-per-group/lan /etc/ocserv/defaults/group.conf

[[ -e /etc/ocserv/ocpasswd ]] || install -m 600 /dev/null /etc/ocserv/ocpasswd
[[ -e /etc/ocserv/users.oath ]] || install -m 600 /dev/null /etc/ocserv/users.oath

# ----- forwarding + nftables ----------------------------------------------
cat >/etc/sysctl.d/90-vpn.conf <<EOF
net.ipv4.ip_forward = 1
EOF
# Unprivileged containers cannot apply every key from sysctl --system.
sysctl -w net.ipv4.ip_forward=1 >/dev/null 2>&1 || true
[[ $(cat /proc/sys/net/ipv4/ip_forward) == 1 ]] || die "IPv4 forwarding is disabled"

cat >/etc/nftables.conf <<EOF
#!/usr/sbin/nft -f
# Managed by projects/home/proxmox/vpn/install-ocserv.sh.
flush ruleset

table inet vpn {
  chain input {
    type filter hook input priority filter; policy drop;
    iif "lo" accept
    ct state established,related accept
    ct state invalid drop
    ip protocol icmp icmp type { echo-request, destination-unreachable, time-exceeded, parameter-problem } accept
    ip6 nexthdr icmpv6 icmpv6 type { echo-request, destination-unreachable, packet-too-big, time-exceeded, parameter-problem, nd-router-advert, nd-neighbor-solicit, nd-neighbor-advert } accept
    iifname "$IFACE" tcp dport 443 accept
    iifname "$IFACE" udp dport 443 accept
    ip saddr $LAN_CIDR iifname "$IFACE" tcp dport 22 accept
  }

  chain forward {
    type filter hook forward priority filter; policy drop;
    # MSS clamp must run before the accept rules: a SYN that is accepted first
    # never reaches the clamp, so clients keep a too-large MTU.
    oifname "$IFACE" tcp flags syn tcp option maxseg size set rt mtu
    ct state established,related accept
    iifname "vpns*" oifname "$IFACE" ip saddr $VPN_CIDR accept
  }

  chain postrouting {
    type nat hook postrouting priority srcnat; policy accept;
    ip saddr $VPN_CIDR oifname "$IFACE" masquerade
  }
}
EOF

nft -c -f /etc/nftables.conf

# ----- SSH: only key-based root login; firewall limits it to LAN -----------
install -d -m 755 /etc/ssh/sshd_config.d
cat >/etc/ssh/sshd_config.d/10-vpn.conf <<EOF
PermitRootLogin prohibit-password
PasswordAuthentication no
KbdInteractiveAuthentication no
EOF

systemctl enable nftables
systemctl restart nftables
systemctl enable ssh
systemctl restart ssh
ocserv --test-config
systemctl enable ocserv
systemctl restart ocserv

pin=$(openssl x509 -in /etc/ocserv/ssl/server-cert.pem -pubkey -noout \
  | openssl pkey -pubin -outform DER \
  | openssl dgst -sha256 -binary | openssl base64)
echo "ocserv installed; server public key pin-sha256:$pin"
