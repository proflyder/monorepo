# ARCHITECTURE — устройство стенда

Дизайн. Текущие адреса/статусы/ограничения — `STATE.md`; команды — `OPERATIONS.md`.

## Компоненты

```
Станция 192.0.2.13
  │  EGRESS_GUARD: private LAN/DHCP/NTP, остальной WAN DROP
  │
  ├── Pulsar v0.4.1-ricardo6 (ESPHome :6053, Sendspin :8928)
  ├── BLE gateway (event2: 1/3/4/5s)
  ├── ADB :5555 / SSH :22 / OTA fortress
  └── Яндекс-карантин (hosts/unbound/iptables/chrony)
  │  Wi-Fi / LAN
  ▼
Mac 192.0.2.11 (Quasar/MITM kit; не активен, нужен для factory reset)
  ├── DNS :53 / https-proxy — не используются (router DNS не на Mac)
  ├── alice-mitm-bootstrap — `archive/legacy/alice-mitm-bootstrap/`
  └── v3rt3p — `archive/legacy/v3rt3p-stack/` + `repos/` (архив;
      upstream: https://github.com/v3rt3p)

Proxmox host 192.0.2.26 (Ethernet; Wi-Fi .25 DOWN)
  ├── VM 100 HAOS 192.0.2.28 — HA/Supervisor :8123
  │     ├── Matter Server add-on :5580 (localhost)
  │     ├── Music Assistant add-on :8095 (stream :8097, Sendspin :8927)
  │     └── ESPHome integration ← Pulsar :6053
  └── VM 101 lab-k3s 192.0.2.27 — Ubuntu + K3s
      ├── HA compatibility HAProxy .27:8123 → HAOS
      ├── STT :10305 GigaAM v3, :10300 Whisper fallback
      ├── Piper TTS :10200 (ru_puriso2-medium)
      ├── Hermes gateway :8642 (loopback) + voice proxy :8643
      └── Tailscale Operator: pl-home → HAOS, /grafana → Grafana
```

> Wi-Fi на Proxmox host пока не включён; Ethernet обязателен до отдельного
> routed/NAT-проекта. HAOS и K3s — отдельные VM, не контейнеры.

## Роли

- **Станция** — Pulsar-спутник HA; штатный Quasar/`maind` остановлен.
  Egress-политика и OTA/ADB fortress сохранены.
- **Mac emulator/Quasar kit** — исторический стенд; `alice-mitm-bootstrap`
  и `v3rt3p-stack` остановлены, но **сохранены** в `archive/legacy/`
  (source + `repos/`). Нужны только для factory reset/повторного root
  станции; HA/Pulsar работают без Mac. Recovery — `REBUILD.md` §4.
- **Неттоп `lab-host`** — always-on хаб: Home Assistant Container + Music
  Assistant + Matter Server + Whisper/Piper + Hermes fallback; Pulsar подключён как ESPHome
  satellite и Sendspin-плеер. Детали — `projects/home-assistant/stack/`, `projects/home-assistant/hermes/`.

## Потоки

### Boot / sync / ADB (исторический stock-путь)

1. Станция резолвит `quasar.yandex.net` → `192.0.2.11` (hosts/DNS).
2. `https-proxy` отдаёт сертификат и проксирует в `quasar-server`.
3. `get_sync_info` возвращает `adbd.network=true`.
4. `activate_adb.sh` создаёт маркеры, keeper поднимает `adbd :5555`.

### Голос (активный)

1. Pulsar: VQE + spotter «Алиса» → ESPHome API `:6053`.
2. HA Assist pipeline `Рикардо`: GigaAM v3 / Whisper fallback → local intents;
   нераспознанное → Hermes `conversation.hermes_agent` через MCP.
3. Piper `ru_RU-irina-medium` → FLAC через `tts_proxy` → Pulsar GStreamer.
4. Pulsar LED/часы показывают состояние; MA — отдельный Sendspin-плеер.
5. Hermes не владеет таймерами/будильником/состоянием музыки: это HA.

### Голос (исторический emulator)

Старый путь `uniproxy → https-proxy → quasar-server → processor` больше не
обслуживает станцию; оставлен как стенд.

### OTA

- `ALLOW_UPDATES=false`; `/check_updates` → `hasUpdate:false`.
- s3/cdn `403`; `updatesd` отсутствует; OTA applier no-op.
- Механизм — `OTA-FORTRESS.md`.

## Egress / карантин

- `EGRESS_GUARD` — station-side allowlist: private LAN, loopback, DHCP,
  multicast, Google NTP; публичный WAN — DROP.
- `yandex-block.sh` — DNS/hosts/chrony/старые Yandex-сети + возврат
  `launch.json` без telemetry.
- DNS-спуфинг и iptables — defense-in-depth; основной барьер — egress-guard.

## Network provisioning (дизайн)

- **Свой BLE gateway** — Go/BlueZ, `Station-Gateway`; SSID/PSK/token/commit/
  status; 1/3/4/5-жесты; HTTP `:8099` для HA; авто при отсутствии Wi-Fi;
  без облака. Детали — `../projects/station/ble-gateway/README.md`.
- **Штатный BLE setup** — app «Дом с Алисой» колонку не видит; не используется.
- **Sound pairing** — FSK-аудио с SSID/PSK; станция принимает микрофоном.
- **AP + captive portal** — станция сама AP; hostapd/DHCP пока нет.
- **DPP/Wi-Fi Easy Connect** — QR → телефон; при активном Wi-Fi драйвер
  блокирует.
- **USB NCM** — исключён: data не подтверждён, питание 5 В не хватает.
- **Fallback SSID** — опционально, phone hotspot; travel router исключён.

Детали и сравнение — `PROVISIONING.md`.

## Сертификаты

- Листья подписываются eMudhra intermediate; цепочка до AAA, доверенного
  станции.
- Ключи — в `secrets/certs/`; не публиковать.
- После factory reset доверие к eMudhra нужно восстанавливать.

## Ограничения

- Эмулятор не покрывает музыку, умный дом, навыки, звонки, Алису Плюс.
- `quasar-server` API без auth (P0 в `PENDING.md`).
- Recovery/factory не защищены OTA-блоком; проверенного Midi-образа нет.
- Полный список текущих ограничений — `STATE.md`.
