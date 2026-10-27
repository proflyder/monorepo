# Артефакты и recovery-ассеты

**Правило:** здесь только то, что нужно для воспроизведения/восстановления.
TTS-исследования — не recovery-critical; Mini 2 — в `archive/legacy/mini2/`.
Точные дубли удалены 2026-10-27; канонические копии ниже.

| Путь | Что | Зачем |
|---|---|---|
| `alice-mitm-bootstrap-2026-10-21.tar.gz` | полный bootstrap kit (Bun-прокси, certs, source) | stage 1: вернуть ADB после factory reset |
| `desktop-originals/alice-mitm-bootstrap-20261026.tar.gz` | архив legacy source | восстановление Mac/Quasar |
| `ssh/dropbear`, `ssh/dropbearkey` | статические ARM32-бинарники | SSH на станции |
| `pcap/station.pcap` | TLS-захват | диагностика bootstrap |
| `firmware/yandexmidi-…zip` | официальная прошивка **Station 2** | **НЕ для Midi**; не прошивать |
| `wakeword/ricardo-v1/` | модель, manifest, конфиг, метрики и зависимости | обучение и восстановление wake word |
| `wakeword/ricardo-station/` | бинарник, debug symbols, source archive, rollback | восстановление Pulsar с microWakeWord |
| `reference/jarvis.zip` | сторонний on-device wake word | reference |
| `prior-session/codex-session-…md` | транскрипт прошлой сессии | исторический; **секреты** |
| `tts-research-2026-10-26/`, `tts-samples/` | TTS-ресерч/сэмплы | research, не recovery |
| `desktop-originals/TTS-voices-20261026.zip` | первый набор TTS | research |
| `desktop-originals/TTS-voices-README.txt` | описание набора | research |

## Важное

- `firmware/yandexmidi` — **Station 2**, не Midi `cucumber`; прошивка не
  совместима. Проверенного factory/recovery образа Midi нет.
- Mini 2 downgrade/rollback перенесены в `archive/legacy/mini2/`; **не запускать
  на Midi**.
- `alice-mitm`/`prior-session` — чувствительные; см. `docs/SENSITIVE-DATA.md`.
- Полный recovery-порядок — `docs/REBUILD.md`.
