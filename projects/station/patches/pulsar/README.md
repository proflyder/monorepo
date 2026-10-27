# Локальные изменения Pulsar

`base.json` фиксирует исходный commit и ветку, `local.patch` — полный снимок
наших изменений, включая новые исходники microWakeWord. Вложенный checkout
`projects/station/upstream/pulsar/` остаётся независимым репозиторием и не изменяется скриптом.

Обновить снимок перед коммитом лаборатории:

```bash
python3 projects/station/scripts/snapshot-pulsar.py
git add projects/station/patches/pulsar
git commit -m "Describe the change"
```

Восстановить в отдельном чистом checkout Pulsar на commit из `base.json`:

```bash
git checkout <base_commit>
git apply /absolute/path/to/projects/station/patches/pulsar/local.patch
```

Патч не применять повторно поверх уже изменённого checkout. Модель и данные
обучения — в `artifacts/wakeword/ricardo-v1/`; текущий статус — `docs/STATE.md`.
