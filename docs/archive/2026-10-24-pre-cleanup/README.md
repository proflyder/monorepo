# Yandex Station Midi — root lab

Локальная лаборатория по тестовой **Станции Миди** (`STATION_SERIAL`): root/ADB,
SSH, MITM-стенд, безопасное обновление прошивки и кастомный бэкенд.

**Обновлено:** 2026-10-24 17:15 MSK
**Статус:** колонка рутирована, прошивка `0.369.1.46...160`, SSH/ADB работают,
OTA-запрет/fortress стоят. Включён **Яндекс-карантин + egress-guard**:
разрешены только LAN/loopback/DHCP/Google NTP, остальной WAN — DROP. Яндекс
недоступен независимо от DNS; роутерный DNS и Mac можно выключать, ADB/SSH по
IP остаются. Телеметрия вырезана, OTA-предложения выключены, s3/cdn закрыты
`403`. Яндекс.Музыка/Лента/облако недоступны; кастомный бэкенд ждёт LLM.

> ⚠️ **Открыто:** API `quasar-server` на `0.0.0.0:31116` **без авторизации**
> (`push-directive`/`PATCH`). Закрыть привязкой к `127.0.0.1` — нужно согласование.

---

## Быстрые факты

| Что | Значение |
|---|---|
| Колонка | Yandex Station Midi, `STATION_SERIAL`, MAC `02:00:00:00:00:02` |
| IP колонки | `192.0.2.13` |
| Прошивка | `0.369.1.46.4778262955.20261013.160` (13.10.2026, до патча) |
| Root | `adb shell id` → `uid=0(root)`; adbd на `:5555` без авторизации |
| SSH | `ssh root@192.0.2.13`, пароль — см. `secrets/CREDENTIALS.md` |
| Mac | `192.0.2.11` (en0), macOS 27.0.1 |
| DNS | локальный `:53`; `DNS_NAMES` = полный список Яндекса → `192.0.2.11` |
| Стек | Docker/Colima; `https-proxy` **Up**, API `:31116` без auth (остальные up) |
| Egress | `EGRESS_GUARD`: private LAN/DHCP/Google NTP; остальной WAN DROP; ADB/SSH живы |
| DPP | QR есть, но при активном Wi-Fi драйвер `p2p_support==0`; только unassociated после USB/fallback |
| Sound | локальный WAV/OGG генератор; режим сопряжения Midi ещё не тестирован |
| LLM | **не настроен** (401), Алиса на кастомном бэкенде не отвечает |

---

## Топология

> **Текущий режим:** Яндекс-карантин + station-side egress-guard. Станция
> выпускает только LAN/DHCP/Google NTP; Яндекс режется по IP. Локальная Алиса
> работает только пока Mac включён; при выключенном Mac доступ к станции — по IP.

```
Станция 192.0.2.13
   │  DNS (через роутер 192.0.2.1)
   ▼
Mac 192.0.2.11 :53  (src/dns.ts, полный список доменов Яндекса)
   │
   ├─ quasar.*, scbh.*, clck.*, uniproxy.*, rpc.alice.*, ...
   ▼
https-proxy :443  (nginx, сертификаты eMudhra, chain до AAA)
   ▼
quasar-server :31115  (эмулятор бэкенда Яндекса, API :31116)
   ├─ processor → OpenAI-совместимая LLM (пока нет ключа)
   ├─ quasistreaming-gigam → STT (GigaAM, патч VAD под aarch64)
   ├─ supertonic-tts → TTS
   └─ postgres
```

Подробнее — `docs/ARCHITECTURE.md`.

---

## Структура папки

```
yandex-station-lab/
├── README.md                 ← этот файл
├── docs/
│   ├── CONTEXT.md            краткая память для /compact
│   ├── REPLICATION-GUIDE.md точный гайд по нашей конфигурации
│   ├── RESOURCES.md          совместимость внешних проектов
│   ├── STATE.md              текущее состояние и версии
│   ├── TIMELINE.md           хронология сессий и решения
│   ├── ARCHITECTURE.md       как устроен стенд
│   ├── OPERATIONS.md         команды: статус, SSH, ADB, switch, Docker
│   ├── OTA-FORTRESS.md       что установлено и как откатить
│   ├── PENDING.md            что осталось сделать
│   ├── PROVISIONING.md       переезд/новая Wi-Fi: recovery-план
│   ├── SECURITY.md           риски и правила безопасности
│   └── PRIOR-SESSION.md      краткий разбор прошлой сессии Codex
├── resources/               внешние проекты/архивы (см. docs/RESOURCES.md)
├── artifacts/
│   ├── firmware/             yandexmidi-0.372...298.zip (Station 2, НЕ Midi)
│   ├── pcap/                 station.pcap (TLS alert unknown_ca)
│   ├── ssh/                  dropbear / dropbearkey (ARM32 static)
│   ├── prior-session/        полный транскрипт прошлой сессии
│   ├── alice-mitm-bootstrap-2026-10-21.tar.gz
│   └── desktop-originals/    исходники, перемещённые с рабочего стола
├── configs/                  снимки .env, compose, скриптов, DNS, certs
├── secrets/                  приватные ключи и пароли (chmod 700/600)
├── scripts/                  station-status.sh, restore-ota-fortress.sh, station-egress-guard*.sh
├── backups/                  оригиналы файлов с колонки
├── logs/                     снимки статуса и логи
├── services/                 симлинки на живые проекты
│   ├── alice-mitm-bootstrap → ~/Desktop/alice-mitm-bootstrap
│   └── v3rt3p-stack         → ~/Desktop/v3rt3p-stack
└── tools/
    ├── quasar-root-kit/      меню, MITM-прокси, OTA/fortress
    └── misc/                 yandex-micro-kit, rollback.sh, README Mini 2
```

Живые проекты **не перемещены**, чтобы не сломать запущенные процессы и
bind-mounts Docker; на них есть симлинки в `services/`.

---

## Быстрые команды

```bash
# статус всего
cd ~/Desktop/v3rt3p-stack && docker compose ps
cd ~/Desktop/alice-mitm-bootstrap && ./scripts/proxy-ctl.sh status
adb connect 192.0.2.13:5555 && adb devices
ssh root@192.0.2.13

# egress-guard: статус/снять (Yandex-блок и OTA остаются)
adb -s 192.0.2.13:5555 shell 'iptables -S EGRESS_GUARD'
adb -s 192.0.2.13:5555 shell /data/quasar/egress-guard-off.sh

# legacy: bootstrap quasar-only; НЕ снимает карантин/egress-guard
cd ~/Desktop/v3rt3p-stack && ./switch-back.sh

# снова на кастомный стек
cd ~/Desktop/v3rt3p-stack && ./switch-to-stack.sh
```

---

## Критично

- Роутер форвардит DNS на этот Mac. Сон/выключение Mac ломает DNS-имена и
  локальную Алису, но **не карантин**: egress-guard на станции продолжает
  блокировать Яндекс. Доступ к станции — по IP из LAN.
- `:5555` даёт root без пароля, `:22` — root по слабому паролю. Только домашняя
  сеть, не пробрасывать наружу; egress-guard не заменяет защиту сервисов.
- В `secrets/` лежат приватные ключи из набора eMudhra. Не публиковать.
- OTA-запрет/fortress стоят: `updatesd` вырезан, `ota_script.pub` подменён,
  `device_ota_update.sh` — no-op, adbd защищён keeper'ом. Factory reset сносит
  `/data` и защиту.
