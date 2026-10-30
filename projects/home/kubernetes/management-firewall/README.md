# K3s management firewall

Single-node policy: allow loopback, the node IPv4 and pod CIDR; drop other
sources to TCP 6443/10250/9100/9116 and UDP 8472, including IPv6 sources.
SSH, Grafana NodePort and application ports are unaffected. This does not
replace the separate voice-proxy firewall or router policy.

1. Render `rules.nft` privately, replacing `NODE_IPV4` with the actual node
   address and `POD_CIDR` with the configured pod network.
2. Install it mode 0600 as `/etc/nftables.d/ricardo-k3s.nft`.
3. Install `apply.sh` mode 0755 as `/usr/local/sbin/ricardo-k3s-firewall` and
   the service in `/etc/systemd/system/`.
4. Run `systemctl daemon-reload` and `systemctl enable --now ricardo-k3s-firewall`.
5. Verify denied ports from LAN, then run cluster tests through an SSH tunnel.

The script validates rules before applying and replaces only its own table
in one nft transaction. Kubernetes tables remain managed by their
owners. Update this policy before adding nodes or changing the node address.
Rollback: disable the service and delete only `table inet ricardo_k3s`.
