#!/bin/sh
# FORTRESS-EGRESS-GUARD v1
# Allow only loopback, local LAN, link-local, multicast/broadcast, DHCP,
# (optionally) Google NTP and (optionally) HTTPS to GitHub for Pulsar's
# updater. Drop all other IPv4/IPv6 egress.
# INPUT is untouched: ADB/SSH and LAN access keep working.
# Teardown: /data/quasar/egress-guard-off.sh

set -u

ALLOW_NTP="${ALLOW_NTP:-1}"
NTP_NET="${NTP_NET:-216.239.35.0/24}"
# github.com and the release asset CDN: IPv4 "web" ranges of api.github.com/meta.
ALLOW_GITHUB="${ALLOW_GITHUB:-1}"
GITHUB_NETS="${GITHUB_NETS:-140.82.112.0/20 143.55.64.0/20 185.199.108.0/22 192.30.252.0/22}"

# ---- IPv4 ----
iptables -N EGRESS_GUARD 2>/dev/null || true

if ! iptables -C EGRESS_GUARD -j DROP 2>/dev/null; then
  iptables -F EGRESS_GUARD 2>/dev/null || true

  iptables -A EGRESS_GUARD -o lo -j ACCEPT
  iptables -A EGRESS_GUARD -d 10.0.0.0/8 -j ACCEPT
  iptables -A EGRESS_GUARD -d 172.16.0.0/12 -j ACCEPT
  iptables -A EGRESS_GUARD -d 192.168.0.0/16 -j ACCEPT
  iptables -A EGRESS_GUARD -d 169.254.0.0/16 -j ACCEPT
  iptables -A EGRESS_GUARD -d 224.0.0.0/4 -j ACCEPT
  iptables -A EGRESS_GUARD -d 255.255.255.255 -j ACCEPT
  iptables -A EGRESS_GUARD -p udp --sport 68 --dport 67 -j ACCEPT
  iptables -A EGRESS_GUARD -p udp --sport 546 --dport 547 -j ACCEPT

  if [ "$ALLOW_NTP" = "1" ]; then
    iptables -A EGRESS_GUARD -p udp --dport 123 -d "$NTP_NET" -j ACCEPT
  fi

  iptables -A EGRESS_GUARD -j DROP
fi

# Inserted at the top, so a chain built by an older version gains them in
# front of its DROP when the script runs again.
if [ "$ALLOW_GITHUB" = "1" ]; then
  for net in $GITHUB_NETS; do
    iptables -C EGRESS_GUARD -d "$net" -p tcp --dport 443 -j ACCEPT 2>/dev/null ||
      iptables -I EGRESS_GUARD 1 -d "$net" -p tcp --dport 443 -j ACCEPT
  done
fi

iptables -C OUTPUT -j EGRESS_GUARD 2>/dev/null || iptables -I OUTPUT 1 -j EGRESS_GUARD

# ---- IPv6 ----
ip6tables -N EGRESS_GUARD6 2>/dev/null || true

if ! ip6tables -C EGRESS_GUARD6 -j DROP 2>/dev/null; then
  ip6tables -F EGRESS_GUARD6 2>/dev/null || true

  ip6tables -A EGRESS_GUARD6 -o lo -j ACCEPT
  ip6tables -A EGRESS_GUARD6 -d fc00::/7 -j ACCEPT
  ip6tables -A EGRESS_GUARD6 -d fe80::/10 -j ACCEPT
  ip6tables -A EGRESS_GUARD6 -d ff00::/8 -j ACCEPT
  ip6tables -A EGRESS_GUARD6 -p udp --sport 546 --dport 547 -j ACCEPT
  ip6tables -A EGRESS_GUARD6 -j DROP
fi

ip6tables -C OUTPUT -j EGRESS_GUARD6 2>/dev/null || ip6tables -I OUTPUT 1 -j EGRESS_GUARD6

exit 0
