# platform/deploy

Общие шаблоны развёртывания. Project-specific compose/env/systemd живут в
`projects/<name>/`.

```text
docker/    reusable Dockerfile fragments
systemd/   reusable unit templates
```

Правила:

- сервисы слушают loopback или LAN, не публикуются в интернет;
- Tailscale Serve/Caddy — единственный внешний вход;
- секреты — через env/secret files;
- `ha-bridge` общается с HA, `alarm` не управляет Pulsar напрямую.
