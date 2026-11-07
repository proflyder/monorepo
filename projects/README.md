# projects

Композиции solutions. Здесь живут project-specific конфиги, env, манифесты,
systemd и deployment, но **не** бизнес-логика.

```text
projects/station/          Pulsar/ADB/BLE/provisioning
projects/home-assistant/   HA/MA/STT/TTS/Hermes
projects/home/             композиция station + home-assistant
projects/mobile/           Android-клиент
```

Один и тот же `solutions/scheduler`/`ha-bridge` запускается с разными БД,
портами и endpoint; project задаёт только композицию.

## Правила

- project знает, какие solutions и какие их инстансы использовать;
- solution не знает про project;
- секреты и endpoint — только в project env/config;
- deploy-файлы проекта не импортируются другими проектами.
