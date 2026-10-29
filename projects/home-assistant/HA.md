# HA — Home Assistant OS VM

Текущий runtime — HAOS VM `100` на Proxmox, не Kubernetes и не Container.
Факты — `../../docs/STATE.md`; команды — `../../docs/OPERATIONS.md`.

## Параметры

- HAOS `2026.10.4`, 2 vCPU / 4 GiB / 32 GiB.
- IP `192.0.2.28`; UI `http://192.0.2.28:8123`.
- `internal_url` и trusted proxy настроены; HAProxy удалён.
- Внешний вход — Tailscale Operator.

## Add-ons

- Music Assistant `2.10.8`: UI/API `8095`, stream `8097`, Sendspin `8927`.
- Matter Server `9.2.0`: `ws://127.0.0.1:5580/ws`; paired 1 узел — Yandex LED
  strip `YNDX-00544` («Лента»).
- Данные: `/mnt/data/supervisor/apps/data/<slug>`.

## HA config / backup

- HA config — внутри HAOS; supported backup через Supervisor.
- HA auth/MFA — только private; не копировать в Git.
- MA/Matter data — в HAOS add-on data; K3s `runtime-music`/`runtime-matter`
  удалены, tar-копии на Mac.

## Проверка

```bash
curl -fsS -o /dev/null -w '%{http_code}\n' http://192.0.2.28:8123/
curl -fsS -o /dev/null -w '%{http_code}\n' http://192.0.2.28:8095/
```

После reboot проверить HA, add-ons, MA player/Sendspin, Matter, Tailscale,
alarm media URLs и voice pipeline.

## Связанные документы

- [`../../docs/STATE.md`](../../docs/STATE.md)
- [`../../docs/OPERATIONS.md`](../../docs/OPERATIONS.md)
- [`../../docs/KUBERNETES-OPERATIONS.md`](../../docs/KUBERNETES-OPERATIONS.md)
- [`../../docs/REBUILD.md`](../../docs/REBUILD.md)
- [`ha-voice/README.md`](ha-voice/README.md)
