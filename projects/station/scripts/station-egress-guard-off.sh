#!/bin/sh

while iptables -D OUTPUT -j EGRESS_GUARD 2>/dev/null; do :; done
iptables -F EGRESS_GUARD 2>/dev/null || true
iptables -X EGRESS_GUARD 2>/dev/null || true

while ip6tables -D OUTPUT -j EGRESS_GUARD6 2>/dev/null; do :; done
ip6tables -F EGRESS_GUARD6 2>/dev/null || true
ip6tables -X EGRESS_GUARD6 2>/dev/null || true

exit 0
