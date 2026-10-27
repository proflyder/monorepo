# projects/home

Композиция домашнего проекта. Объединяет два независимых проекта:

```text
projects/station          Pulsar/ADB/BLE/provisioning
projects/home-assistant   HA/MA/STT/TTS/Hermes/Tailscale
```

И добавляет домашние solutions:

```text
solutions/alarm           instance=home
solutions/ha-bridge       endpoint=home alarm
solutions/mcp-alarm       endpoint=home alarm
solutions/assistant-ui    endpoint=home alarm
```

## Зачем отдельно

- станция и HA развиваются независимо;
- HA можно перезапускать/обновлять без перепрошивки станции;
- один и тот же alarm/MCP можно направить на другой инстанс;
- project-specific config не попадает в solutions.

Рабочая Kubernetes-композиция и pinned Helm releases — [kubernetes/](kubernetes/README.md).
HA Core перенесён в HAOS VM; K3s VM содержит MA/Matter/STT/TTS/Hermes и
observability. Старые HA data paths сохранены как архив/PV, но не являются
текущим writer. Alarm/MCP/UI solutions остаются отдельными дополнениями.
