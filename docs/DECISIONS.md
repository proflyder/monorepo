# DECISIONS — ключевые решения и почему так

**Назначение:** короткая память «почему именно это, а не иначе». Текущие факты —
`STATE.md`; восстановление — `REBUILD.md`; секреты — `SENSITIVE-DATA.md`.

| # | Решение | Почему | Альтернативы / почему нет |
|---|---|---|---|
| 1 | **Pulsar как HA-спутник, штатная Алиса остановлена** | полный контроль, локальный STT/TTS, HA Assist, нет облака Яндекса; станция остаётся в карантине | штатная Алиса/`maind` — зависимость от Яндекса, телеметрия, OTA/сброс; гибрид Pulsar+maind не поддерживается |
| 2 | **HA Container, не HAOS** | на неттопе уже Ubuntu; нужен Docker-стек; полный HA Core; host network для mDNS/ESPHome/MA | HAOS VM — добавить, если понадобятся add-ons; Container проще и уже работает |
| 3 | **Remote HA — Tailscale + `tailscale serve`, не публичный HA** | нет port-forward, нет публичного `8123`, валидный TLS `*.ts.net`, работает за double NAT; ключи не вручную | публичный Caddy/8123 — больше attack surface; Funnel/port-forward — нет; Link/Cloudflare — платно/третья сторона; WireGuard — ключи вручную |
| 4 | **MFA/TOTP в HA, ACL только `tcp:443`** | VPN не заменяет auth приложения; tag/ACL закрывает SSH/MA/Zigbee на tailnet | «только пароль» — недостаточно; открытый tailnet — лишняя поверхность |
| 5 | **STT/TTS локально на неттопе** | приватность, русский язык, нет облачных расходов/задержек; Whisper/GigaAM + Piper; выбранный голос — в `STATE.md` | облачные STT/TTS — быстрее старт, но приватность/цена/сеть; Hermes — только fallback |
| 6 | **Hermes только как fallback** | локальные intents (таймер, будильник, музыка, громкость) должны работать без LLM; Hermes — свободная речь/web | всё через LLM — латентность, цена, ненадёжность; Hermes не владеет alarm/timer state |
| 7 | **Music Assistant + Spotify Connect/Soloist** | MA — локальный хаб/плейлисты; Connect — телефон как источник; станция = Sendspin-плеер | только Spotify Connect — нет локальной библиотеки/голосового поиска; только MA — нет phone→speaker; нативный Spotify на станции — нет |
| 8 | **BLE gateway как аварийный provisioning** | работает без Wi-Fi/облака; 1/3/4/5; GATT SSID/PSK/token; auto-mode | штатный BLE setup Яндекса — app не видит колонку; AP/captive portal — сложнее; sound pairing — offline fallback |
| 9 | **OTA/ADB fortress + egress-quarantine** | защита от удалённого OTA/сброса/закрытия ADB; станция не ходит в WAN | без блока — риск потери root/ADB; factory reset сносит защиту, поэтому recovery kit |
| 10 | **Quasar/MITM kit сохранён, но не активен** | factory reset возвращает сток; stage 1 fake `get_sync_info` возвращает ADB; stage 2/v3rt3p — fake backend | держать активным — DNS/proxy на Mac = единая точка отказа; выбросить — потерять путь восстановления ADB |
| 11 | **Backup: локальный + offsite; factory-образа Midi нет** | SSD/сервер может умереть; HA config/secrets критичны | считать OTA-блок заменой recovery — нет; factory reset не unbrick; `yandexmidi` — Station 2, не Midi |
| 12 | **Mac с Quasar — не часть runtime** | HA/MA/STT/TTS/Pulsar работают без Mac; Mac нужен только для root/восстановления станции | всегда-on Mac — лишняя зависимость; Mac DNS/proxy без autostart |
| 13 | **Расписания — HA-native списки, не Yandex alarms** | текущий рабочий контур: будильники в `local_todo`, таймеры в `timer` helpers, напоминания/заметки в `local_todo`; голос + dashboard; старый один слот скрыт | нужен UI CRUD, несколько объектов, persistence; native Yandex alarm API в стеке нет; HACS alarm-интеграции слишком свежие для критичного пробуждения |
| 14 | **Горизонтальный монорепозиторий + Bazel, alarm на Go** | проект растёт до приложений на Go/Rust/mobile; Bazel даёт единый hermetic build; `solutions/alarm` на Go — always-on gRPC/Connect scheduler с одним бинарником; MCP и HA bridge — отдельные solutions; Rust — для DSP/инференса | отдельные репозитории/скрипты — быстро сейчас, дорого при росте; Rust для control plane — лишняя сложность, Go для DSP — не лучший инструмент |

## Решение по мужскому TTS, 27 октября 2026

Пользователь предпочёл качество речи Piper и выбрал дополнительный тембр Puriso. Поэтому сохраняем существующий Piper/Wyoming, а собственный голос готовим следующим отдельным этапом. Старый shortlist Silero/Eugene не является текущим планом внедрения. Протокол выбора и проверок — [DEPLOY-RICARDO-PURISO.md](DEPLOY-RICARDO-PURISO.md); текущие факты — `STATE.md`.

## Границы

- Решения 1–9 описывают **текущий** стенд; не менять без `HISTORY.md`.
- Решение 10 — **recovery-only**; не запускать без отдельной задачи.
- Решение 3 — Tailscale; альтернативы сохранены в
  `docs/archive/2026-10-26-remote-access/`.
- Если меняется решение — обновить эту таблицу, `STATE.md` и `REBUILD.md`.
