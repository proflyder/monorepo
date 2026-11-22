# Hermes voice-proxy source restriction

HAOS reaches the K3s Hermes voice
proxy directly at `<K3S_VM_IP>:8643`. Because the proxy is not an
authentication boundary, the K3s VM must allow only HAOS, the K3s node itself
and loopback.

Copy the examples to the K3s VM, replace the two example addresses in
`voice.nft`, then install:

```bash
sudo install -m 0644 voice.nft /etc/nftables.d/home-voice.nft
sudo install -m 0755 voice-firewall /usr/local/sbin/home-voice-firewall
sudo install -m 0644 voice-firewall.service \
  /etc/systemd/system/home-voice-firewall.service
sudo systemctl daemon-reload
sudo systemctl enable --now home-voice-firewall.service
```

Verify:

```bash
sudo nft list table inet home_voice
# from HAOS:
curl -sS -o /dev/null -w '%{http_code}\n' http://<K3S_VM_IP>:8643/v1/models
# from another LAN host: connection should time out
```

The script is safe to rerun. Rollback: disable the service and delete
`table inet home_voice`.
