# Что осталось

**Текущий режим:** Яндекс-карантин + station-side `EGRESS_GUARD`. Разрешены
только LAN/loopback/DHCP/Google NTP; остальной WAN DROP. Яндекс режется по IP
независимо от DNS; роутерный DNS и Mac можно выключать. ADB/SSH и доступ из LAN
сохранены. Телеметрия вырезана, OTA-предложения и s3/cdn закрыты.

## P0 — закрыть API 31116

`quasar-server` публикует `0.0.0.0:31116` **без авторизации**:

- `POST /devices/:duid/push-directive` — произвольная директива;
- `PATCH /devices/:duid` — `quasarConfig` + `pushUpdateConfig`.

Любой в LAN может отправить станции директиву/конфиг. Предложено:

```yaml
ports:
  - "127.0.0.1:31116:31116"
```

или убрать публикацию. Затем `docker compose up -d quasar-server`.
Станция/прокси используют внутренний `quasar-server:31115`, Алиса не пострадает.
**Нужно согласование.**

## P1 — network provisioning / переезд

- **Guard:** private LAN уже разрешён (`10/8`, `172.16/12`, `192.168/16`,
  `100.64/10`, `fc00::/7`); ADB/SSH/API проверены, Яндекс закрыт.
- **DPP установлен, но с активным Wi-Fi не работает:** драйвер `wlan0` —
  `vm_p2p_support==0`; `p2p0` off-channel работает, но роняет `wlan0`
  (восстановили reboot). **p2p0 при рабочей сети не запускать.** DPP — только
  unassociated после USB/fallback; Android 10+ вероятен, iPhone не гарантирован.
  Force-listen: `touch /data/quasar/dpp-force-listen`; снять — `rm`.
- **Sound:** локальный генератор `scripts/station-pairing-sound.py` (WAV/OGG,
  без Telegram/облака); тесты 15/15. **Нужен режим сопряжения Midi** и реальный
  прогон; случайный токен только Wi-Fi, аккаунт не привяжет.
- **Fallback SSID/USB/AP ещё не внедрены:** нет профиля `Station-Recovery`,
  USB NCM не проверен кабелем, AP требует hostapd/DHCP.
- **Не переезжать и не менять Wi-Fi** до теста DPP/sound и появления
  fallback/USB. План — `docs/PROVISIONING.md`.

## P1 — factory/recovery

- Проверенного **factory-образа Midi `cucumber` нет**.
- В `artifacts/firmware/` лежит `yandexmidi-0.372.1.8.20261019.298.zip` — это
  **Station 2 (`yandexmidi`)**, не Midi. **Не флешить.**
- Raw-дампа `bootloader` / `boot` / `dtb` / `recovery` (192 МиБ) /
  `aml_sysrecovery` (1.5 ГиБ) нет. Дамп не делали.
- Factory reset снесёт `/data`, root/SSH/OTA/telemetry-скрипты; от серверного
  лока/`stolen` не гарантирует. Не использовать как unbrick без дампа.
- Следующий шаг (по согласованию): read-only дамп разделов на Mac + проверка,
  что recovery вообще запускается. Только на запасной Midi.

## P1 — Mac DNS/прокси (только для локальной Алисы)

- DNS `src/dns.ts` запущен через `nohup`; LaunchAgent/автозапуска нет.
- `https-proxy` — Docker с `restart: unless-stopped`, но Colima/Docker должны
  быть запущены.
- Сон/ребут Mac ломает локальную Алису/DNS, **но не карантин**: egress-guard
  живёт на станции. Доступ к станции — по IP из LAN.
- Нужен LaunchAgent для DNS (и, возможно, автозапуск Colima/compose) либо
  always-on устройство, если нужна локальная Алиса при выключенном Mac.
- Роутер сейчас Static DNS → Mac; для карантина только колонки его можно
  вернуть `192.0.2.1`. Остальные устройства перестанут зависеть от Mac.

## P2 — ресурсы/кандидаты

Всё лежит в `resources/`, матрица — `docs/RESOURCES.md`. Ничего не установлено.

- `pulsar` — Midi, но требует Home Assistant/ESPHome и **заменяет** стек
  Яндекса; проверен на `.351`, у нас `.160`. Только отдельный тест.
- `yandex-station-repo` / `apkg` — aarch64/buildroot, возможен; ставит в rootfs
  `/usr/local`/`/opt`; тест на запасной.
- `yaff` — YAM, Midi support, нужна сборка; web UI без auth.
- `yse` — по описанию лучший для Midi (`/data/opt`), но **нет исходника/ссылки**;
  нужен архив/URL.
- `spotiya` — только Mini 2/armv7; несовместим.
- AirPlay 2 — `cucumber` в таблице, но тест только Mini 3; отключает Алису.
- `spotify.sh` — битый, нет `setup.sh`/`librespot-static`; не запускать.
- `yandex-station-1-panel` — Station 1; OTA decrypt — Mini 2/Lite/Station 2;
  к Midi не применимы.

## P2 — кастомный бэкенд

- LLM: нужен OpenAI-совместимый endpoint/ключ/модель; заполнить
  `LLM_BASE_URL/LLM_API_KEY/LLM_MODEL` в `~/Desktop/v3rt3p-stack/.env`,
  `docker compose up -d processor`.
- Функции/состояние: `PROCESSOR_FUNCTION_SERVER_URLS`,
  `PROCESSOR_STATE_SERVER_URLS` пустые; музыка/умный дом — отдельные интеграции.
- Полной совместимости с Алисой нет.

## P2 — SSH/секреты

- SSH по ключам вместо пароля из `secrets/CREDENTIALS.md`; отключить password auth.
- Сменить пароль на длинный.
- `secrets/` и PoC-ключи не публиковать; бэкап вне Mac — зашифрованным архивом.

## Бэкапы

- `/data/opt/backup` на колонке.
- `lab/backups/emergency-20261023-2137/` — полный бэкап (151 МБ).
- `lab/backups/pre-telemetry-20261023-2223/` — старые compose/generate.
- `lab/backups/station-originals-20261023/` — оригиналы OTA/ADB.
- `lab/backups/egress-20261024-050346/` — iptables-save и старые onboot/keeper.
- Сырые recovery/factory разделы **не сохранены** — см. P1.

## Egress-guard — сделано

- `EGRESS_GUARD`/`EGRESS_GUARD6`: LAN/loopback/DHCP/multicast + Google NTP,
  остальной IPv4/IPv6 DROP.
- Скрипты: `/data/quasar/egress-guard*.sh`; локально — `scripts/station-egress-guard*.sh`.
- Вызов: `onboot.sh` после `yio_iptables_init.sh`; keeper раз в 60 с.
- Проверено: reboot, новое ADB/SSH, LAN API, блокировка Яндекса и `1.1.1.1`,
  NTP Google; Mac DNS/proxy выключены — доступ сохранился.
- Откат: `/data/quasar/egress-guard-off.sh` (Yandex-блок/OTA не снимает).
- Ограничения: доверие LAN `192.0.2.0/24`; NTP — единственное WAN-исключение.
  При выключенном Mac локальной Алисы нет, доступ — с другого LAN-устройства.

## OTA/fortress — сделано

См. `docs/OTA-FORTRESS.md`. Откат — `scripts/restore-ota-fortress.sh`.
Не запускать, если не нужно обновляться.
