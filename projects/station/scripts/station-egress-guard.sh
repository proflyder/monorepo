#!/bin/sh
# FORTRESS-EGRESS-GUARD v1
# Allow only loopback, local LAN, link-local, multicast/broadcast, DHCP and
# (optionally) Google NTP. Drop all other IPv4/IPv6 egress.
# INPUT is untouched: ADB/SSH and LAN access keep working.
# Teardown: /data/quasar/egress-guard-off.sh

set -u

ALLOW_NTP="${ALLOW_NTP:-1}"
NTP_NET="${NTP_NET:-216.239.35.0/24}"

# ---- IPv4 ----
iptables -N EGRESS_GUARD 2>/dev/null || true

if ! iptables -C EGRESS_GUARD -j DROP 2>/dev/null; then
  iptables -F EGRESS_GUARD 2>/dev/null || true

  iptables -A EGRESS_GUARD -o lo -j ACCEPT
  iptables -A EGRESS_GUARD -d 10.0.0.0/8 -j ACCEPT
  iptables -A EGRESS_GUARD -d 172.16.0.0/12 -j ACCEPT
  iptables -A EGRESS_GUARD -d 192.168.0.0/16 -j ACCEPT
  iptables -A EGRESS_GUARD -d 100.64.0.0/10 -j ACCEPT
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
