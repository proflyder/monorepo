#!/bin/sh
set -eu
# Validate first; replace our table atomically without flushing Kubernetes rules.
/usr/sbin/nft -c -f /etc/nftables.d/home-k3s.nft
if /usr/sbin/nft list table inet home_k3s >/dev/null 2>&1; then
  { echo 'delete table inet home_k3s'; cat /etc/nftables.d/home-k3s.nft; } | /usr/sbin/nft -f -
else
  /usr/sbin/nft -f /etc/nftables.d/home-k3s.nft
fi
