# Архитектура

## Компоненты

```
Станция 192.0.2.13
  │  EGRESS_GUARD: LAN/loopback/DHCP/NTP only; остальной WAN DROP
  │
  │  DHCP DNS = 192.0.2.1 (роутер)
  ▼
Роутер 192.0.2.1
  │  Static DNS / forwarding → 192.0.2.11
  ▼
Mac 192.0.2.11
  ├── DNS :53            src/dns.ts (Bun)
  │     • A для доменов Яндекса → 192.0.2.11
  │     • AAAA/proчие для них → NODATA
  │     • остальное → 1.1.1.1, 8.8.8.8
  │     • self-exclusion: запросы самого Mac не подменяются
  │
  ├── https-proxy :80/:443   nginx (v3rt3p)
  │     • сертификаты, подписанные eMudhra intermediate (EM EV TLS CA - G3C)
  │     • цепочка до AAA Certificate Services (доверен колонке)
  │     • проксирует домены из domains/yandex.txt в quasar-server
  │
  ├── quasar-server :31115 (внутри) / API :31116 (без auth)
  │     • эмулятор quasar.yandex.net, scbh, clck, oauth, rpc.alice, uniproxy
  │     • get_sync_info: adbd.remoteActivation=true, network=true
  │     • check_updates: ALLOW_UPDATES=false → hasUpdate:false
  │     • API push-directive/PATCH без авторизации — открытый P0
  │
  ├── processor :8080 (внутри)
  │     • LLM (OpenAI-совместимый endpoint)
  │     • встроенные директивы: громкость, Bluetooth
  │     • внешние функции/состояние — пусто (PROCESSOR_*_URLS)
  │
  ├── quasistreaming-gigam :8080 (внутри)
  │     • STT GigaAM v3; VAD TenVAD (на aarch64 → fallback whole-audio)
  │
  ├── supertonic-tts :9150 (внутри)
  │     • TTS, голоса M1–M5 / F1–F6
  │
  └── postgres :5432 (внутри)
        • состояние устройств, конфиг, сессии
```

## Потоки

### Boot / sync / ADB
1. Колонка после ребута резолвит `quasar.yandex.net` → `192.0.2.11`.
2. `https-proxy` отдаёт сертификат, проксирует в `quasar-server`.
3. `get_sync_info` возвращает конфиг с `system_config.adbd.network=true`,
   `remoteActivation=true`.
4. `maind` вызывает `activate_adb.sh --quasmodrom activate --network`,
   создаются маркеры `/data/quasar/activate_adb_*`, поднимается `adbd :5555`.

### OTA (сейчас выключен)
1. `ALLOW_UPDATES=false`; `check_updates` для `cucumber` → `hasUpdate:false`.
2. `*.s3.yandex.net` / `*.svc.cdn.yandex.net` в `https-proxy` → `403`,
   скачивание OTA через стенд не проходит.
3. `updatesd` отсутствует, `device_ota_update.sh` — no-op, `ota_script.pub` —
   dummy. Защита от OTA — на устройстве; recovery/factory ею не защищены.

### Голос (кастомный бэкенд)
1. Колонка открывает WebSocket к `uniproxy.alice.yandex.net` → `https-proxy`
   → `quasar-server`.
2. Аудио идёт в `quasistreaming-gigam` (STT) → текст.
3. `processor` формирует запрос к LLM, получает JSON с `function_calls`/`text`.
4. `supertonic-tts` синтезирует ответ; `quasar-server` отправляет директивы
   и аудио на колонку.

## Текущий режим

**Яндекс-карантин + egress-guard:** на станции `EGRESS_GUARD` разрешает только
LAN/loopback/DHCP/Google NTP, остальной IPv4/IPv6 `OUTPUT` — `DROP`. Поэтому
Яндекс недоступен даже если DNS вернёт реальный IP, а DNS на роутере/Mac
выключен. `https-proxy` на Mac отдаёт локальный бэкенд, пока Mac включён.
Облачная Алиса/Музыка/Лента не работают. `switch-to-stack.sh`/`switch-back.sh` —
legacy-скрипты; они не снимают `EGRESS_GUARD`/iptables/`yandex-block.sh` и не
возвращают реальный Яндекс сами по себе.

## Сертификаты

- `https-proxy` генерирует EC P-384 листья на каждый домен, подписывает
  ключом `EM EV TLS CA - G3C` (из набора eMudhra).
- Цепочка: лист → `EM EV TLS CA - G3C` → `AAA Certificate Services`.
- Колонка доверяет AAA; после factory reset доверие к eMudhra восстановлено.
- В `secrets/certs/` — приватные ключи; не публиковать.

## Защита от петель

- DNS-сервер не подменяет запросы самого Mac (self-exclusion).
- В `/etc/hosts` Mac закреплён реальный IP `uniproxy.alice.yandex.net`.
- `https-proxy` для `*.s3.yandex.net` / `*.svc.cdn.yandex.net` отдаёт `403`;
  в настоящий Яндекс эти домены больше не проксируются.
- `server.py` из quasar-root-kit (если использовать) резолвит upstream через
  DoH и пиннит IP.

## Ограничения эмулятора

- Реализованы не все эндпоинты: `/health`, `/report`,
  `/set_device_capability`, `/push_subscribe` могут отдавать 500.
- Нет музыкального сервиса, умного дома, навыков, звонков, Алисы Плюс.
- LLM — только диалог + встроенные директивы (громкость, Bluetooth).
