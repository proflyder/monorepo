# Hermes voice-proxy source restriction

The HAProxy compatibility layer is retired. HAOS reaches the K3s Hermes voice
proxy directly at `<K3S_VM_IP>:8643`. Because the proxy is not an
authentication boundary, the K3s VM must allow only HAOS, the K3s node itself
and loopback.

Copy the examples to the K3s VM, replace the two example addresses in
`ricardo-voice.nft`, then install:

```bash
sudo install -m 0644 ricardo-voice.nft /etc/nftables.d/ricardo-voice.nft
sudo install -m 0755 ricardo-voice-firewall /usr/local/sbin/ricardo-voice-firewall
sudo install -m 0644 ricardo-voice-firewall.service \
  /etc/systemd/system/ricardo-voice-firewall.service
sudo systemctl daemon-reload
sudo systemctl enable --now ricardo-voice-firewall.service
```

Verify:

```bash
sudo nft list table inet ricardo_voice
# from HAOS:
curl -sS -o /dev/null -w '%{http_code}\n' http://<K3S_VM_IP>:8643/v1/models
# from another LAN host: connection should time out
```
