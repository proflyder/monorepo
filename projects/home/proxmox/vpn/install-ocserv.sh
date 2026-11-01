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
CLIENT_CA=
CLIENT_CRL=
CLIENT_CA_DEST=/etc/ocserv/ssl/client-ca.pem
CLIENT_CRL_DEST=/etc/ocserv/ssl/client-ca.crl
# Optional restricted "ci" group: its own pool and a single allowed target.
CI_CIDR=10.77.1.0/24
CI_HOST=

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
  --client-ca PATH      client CA certificate (PEM); also looks for
                        PATH-derived CRL unless --client-crl is given
  --client-crl PATH     CRL for the client CA (PEM)
  --ci-cidr CIDR        pool for the restricted "ci" group (default 10.77.1.0/24)
  --ci-host ADDRESS     the only host the "ci" group may reach (K3s API/6443)

Client certificate login is enabled only when a client CA is supplied (or a
previously installed /etc/ocserv/ssl/client-ca.pem exists). Password + TOTP
stays enabled in every case.

With --ci-host a certificate whose OU is `ci` gets its own pool, a route only
to ADDRESS/32, no DNS, and firewall rules that allow only ADDRESS tcp/443
(Traefik, for OpenBao over HTTPS) and tcp/6443 (the K3s API, used by kubectl;
no SSH). The general `lan`/`full` pool is never widened to the ci pool.
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
    --client-ca) CLIENT_CA=$2; shift 2 ;;
    --client-crl) CLIENT_CRL=$2; shift 2 ;;
    --ci-cidr) CI_CIDR=$2; shift 2 ;;
    --ci-host) CI_HOST=$2; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) usage >&2; die "unknown argument: $1" ;;
  esac
done

[[ $(id -u) == 0 ]] || die "run inside the container as root"
[[ -n $LAN_CIDR && -n $DNS && -n $SERVER_IP ]] || {
  usage >&2
  die "--lan-cidr, --dns and --server-ip are required"
}
[[ $CI_CIDR =~ ^[0-9.]+/24$ ]] || die "--ci-cidr must be an IPv4 /24: $CI_CIDR"
if [[ -n $CI_HOST ]]; then
  [[ $CI_HOST =~ ^[0-9]+\.[0-9]+\.[0-9]+\.[0-9]+$ ]] ||
    die "--ci-host must be an IPv4 address: $CI_HOST"
fi

export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates gnutls-bin nftables ocserv openssh-server

# ----- optional client certificate authentication --------------------------
# A re-run without --client-ca keeps an already installed CA, so the script
# stays idempotent. A fresh container without a CA keeps password + TOTP only.
if [[ -z $CLIENT_CA && -s $CLIENT_CA_DEST ]]; then
  CLIENT_CA=$CLIENT_CA_DEST
  [[ -n $CLIENT_CRL ]] || CLIENT_CRL=$CLIENT_CRL_DEST
fi
CERT_AUTH_CONF=
if [[ -n $CLIENT_CA ]]; then
  [[ -s $CLIENT_CA ]] || die "client CA not found: $CLIENT_CA"
  [[ -n $CLIENT_CRL ]] || die "--client-crl is required with --client-ca"
  [[ -s $CLIENT_CRL ]] || die "client CRL not found: $CLIENT_CRL"
  openssl x509 -in "$CLIENT_CA" -noout >/dev/null 2>&1 ||
    die "client CA is not a PEM certificate: $CLIENT_CA"
  if [[ $CLIENT_CA != "$CLIENT_CA_DEST" ]]; then
    install -m 644 "$CLIENT_CA" "$CLIENT_CA_DEST"
  fi
  if [[ $CLIENT_CRL != "$CLIENT_CRL_DEST" ]]; then
    install -m 644 "$CLIENT_CRL" "$CLIENT_CRL_DEST"
  fi
  # ocserv 1.3 resolves the certificate method through the security module;
  # it only works as the primary method. Password + TOTP therefore becomes
  # the alternative method: clients with a valid certificate skip it, clients
  # without one still log in with password + TOTP.
  CERT_AUTH_CONF=$(cat <<EOF
auth = "certificate"
enable-auth = "plain[passwd=/etc/ocserv/ocpasswd,otp=/etc/ocserv/users.oath]"
ca-cert = $CLIENT_CA_DEST
crl = $CLIENT_CRL_DEST
cert-user-oid = 2.5.4.3
# Groups (lan/full) are carried as OU values in the client certificate.
cert-group-oid = 2.5.4.11
EOF
)
fi

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
# Authentication directives are non-reloadable in ocserv: compare them so the
# service is restarted only when they actually change.
old_auth=$(grep -E '^(auth|enable-auth|ca-cert|crl|cert-user-oid|cert-group-oid) =' \
  /etc/ocserv/ocserv.conf 2>/dev/null || true)
cat >/etc/ocserv/ocserv.conf <<EOF
# Managed by projects/home/proxmox/vpn/install-ocserv.sh. Do not edit by hand.

# Password + TOTP. Passwords live in ocpasswd, TOTP keys in users.oath.
# When a client CA is configured, certificate authentication is primary and
# password + TOTP is the alternative (see CERT_AUTH_CONF above).
${CERT_AUTH_CONF:-auth = "plain[passwd=/etc/ocserv/ocpasswd,otp=/etc/ocserv/users.oath]"}

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
# CI deploys run one VPN session per parallel matrix job with one certificate.
max-same-clients = 10
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
EOF

cat >/etc/ocserv/config-per-group/full <<EOF
# Full tunnel: all client traffic exits through home.
route = default
dns = $DNS
EOF

cp /etc/ocserv/config-per-group/lan /etc/ocserv/defaults/group.conf

# ----- restricted "ci" group ----------------------------------------------
# A machine client (CI runner) gets its own pool so the general accept rule for
# $VPN_CIDR never covers it. It is routed only to the K3s node and no DNS is
# pushed, so even a leaked certificate cannot reach other home services.
if [[ -n $CI_HOST ]]; then
  cat >/etc/ocserv/config-per-group/ci <<EOF
# Restricted CI profile: own pool, one route, no DNS.
ipv4-network = ${CI_CIDR%/*}
ipv4-netmask = 255.255.255.0
route = $CI_HOST/32
EOF
fi

[[ -e /etc/ocserv/ocpasswd ]] || install -m 600 /dev/null /etc/ocserv/ocpasswd
[[ -e /etc/ocserv/users.oath ]] || install -m 600 /dev/null /etc/ocserv/users.oath

# ----- forwarding + nftables ----------------------------------------------
cat >/etc/sysctl.d/90-vpn.conf <<EOF
net.ipv4.ip_forward = 1
EOF
# Unprivileged containers cannot apply every key from sysctl --system.
sysctl -w net.ipv4.ip_forward=1 >/dev/null 2>&1 || true
[[ $(cat /proc/sys/net/ipv4/ip_forward) == 1 ]] || die "IPv4 forwarding is disabled"

CI_NFT_FORWARD=
CI_NFT_NAT=
if [[ -n $CI_HOST ]]; then
  CI_NFT_FORWARD=$(cat <<EOF
    # CI pool: two targets only, the K3s API and HTTPS (Traefik, used for
    # OpenBao CI auth). This rule sits before the general pool accept; the
    # pools do not overlap.
    ip saddr $CI_CIDR iifname "vpns*" oifname "$IFACE" ip daddr $CI_HOST tcp dport { 443, 6443 } accept
    ip saddr $CI_CIDR drop
EOF
)
  CI_NFT_NAT=$(cat <<EOF
    ip saddr $CI_CIDR oifname "$IFACE" masquerade
EOF
)
fi

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
${CI_NFT_FORWARD}
  }

  chain postrouting {
    type nat hook postrouting priority srcnat; policy accept;
    ip saddr $VPN_CIDR oifname "$IFACE" masquerade
${CI_NFT_NAT}
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
new_auth=$(grep -E '^(auth|enable-auth|ca-cert|crl|cert-user-oid|cert-group-oid) =' \
  /etc/ocserv/ocserv.conf)
if systemctl is-active --quiet ocserv; then
  if [[ $old_auth == "$new_auth" ]]; then
    # Reloading keeps established sessions; everything else is re-read.
    systemctl reload ocserv
  else
    # Authentication methods cannot be changed with reload.
    systemctl restart ocserv
    echo "authentication methods changed; ocserv restarted"
  fi
else
  systemctl restart ocserv
fi

pin=$(openssl x509 -in /etc/ocserv/ssl/server-cert.pem -pubkey -noout \
  | openssl pkey -pubin -outform DER \
  | openssl dgst -sha256 -binary | openssl base64)
echo "ocserv installed; server public key pin-sha256:$pin"
