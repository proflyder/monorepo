# projects

Композиции solutions. Здесь живут project-specific конфиги, compose, env,
systemd и deployment, но **не** бизнес-логика.

```text
projects/station/          Pulsar/ADB/BLE/provisioning
projects/home-assistant/   HA/MA/STT/TTS/Hermes/Tailscale
projects/home/             композиция station + home-assistant + alarm
projects/cloud/            отдельный alarm-инстанс и MCP без HA
projects/mobile/           будущие мобильные клиенты
```

Один и тот же `solutions/alarm` может быть запущен в `home` и `cloud` с
разными БД, портами и endpoint. Один и тот же `solutions/mcp-alarm` может
смотреть на любой из них.

## Правила

- project знает, какие solutions и какие их инстансы использовать;
- solution не знает про project;
- секреты и endpoint — только в project env/config;
- deploy-файлы проекта не импортируются другими проектами.
