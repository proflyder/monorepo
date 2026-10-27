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
Физические пути станции не менялись; существующие HA data paths подключены как local PV.
Alarm/MCP/UI solutions остаются отдельными планируемыми дополнениями.
