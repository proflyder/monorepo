# Локальные изменения Rust Pulsar

`base.json` фиксирует pinned upstream base и текущий HEAD (сейчас
`v0.5.3` = `504830d`, HEAD `ccef054`, тег `v0.5.1002`, ветка `main`; детали — в
самом `base.json`), `local.patch` —
полный delta от базы до worktree: committed, staged, unstaged и новые
исходники. Активный checkout `projects/station/upstream/pulsar-rust/` скрипт
не изменяет.

Обновить снимок перед корневым коммитом:

```bash
python3 projects/station/scripts/snapshot-pulsar.py
git add projects/station/patches/pulsar-rust
git commit -m "Describe the change"
```

Скрипт отказывается писать патч, если с базы появились бинарники, секреты
или слишком крупные новые файлы. Проверки — `scripts/test_snapshot_pulsar.py`.

Восстановить в отдельном чистом checkout на `base_commit` из `base.json`:

```bash
git clone <pulsar-rust-checkout> /tmp/pulsar-restore
cd /tmp/pulsar-restore
git checkout <base_commit>
git apply --binary /absolute/path/to/projects/station/patches/pulsar-rust/local.patch
```

Для снимка, снятого на чистом checkout, `git add -A && git write-tree`
совпадает с `HEAD^{tree}` исходного репозитория. Модель и данные обучения —
в `artifacts/wakeword/ricardo-v1/`; текущий статус — `docs/STATE.md`.
