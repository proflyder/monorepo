# projects

Композиции solutions. Здесь живут project-specific конфиги, env, манифесты,
systemd и deployment, но **не** бизнес-логика.

```text
projects/station/          Pulsar, BLE gateway, конфиги и инструменты станции
projects/home-assistant/   HA, голос, Hermes, STT/TTS, observability HA
projects/home/             Proxmox, K3s/Flux-манифесты, VPN, бэкапы, Windows-ПК
projects/mobile/           Android-клиент
```

Один и тот же `solutions/scheduler`/`ha-bridge` запускается с разными БД,
портами и endpoint; project задаёт только композицию.

## Правила

- project знает, какие solutions и какие их инстансы использовать;
- solution не знает про project;
- секреты и endpoint — только в project env/config;
- deploy-файлы проекта не импортируются другими проектами.
