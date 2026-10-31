# DECISIONS — ключевые решения

Факты — `STATE.md`; восстановление — `REBUILD.md`; секреты — `SENSITIVE-DATA.md`.

1. **Pulsar вместо штатной Алисы.** Полный контроль, локальный STT/TTS,
   HA Assist, egress-quarantine. `maind` остановлен; гибрид не поддерживается.
2. **Proxmox + HAOS VM + Ubuntu/K3s VM.** HAOS — Supervisor/add-ons;
   K3s — voice/Hermes/observability. Terraform — IaC, отдельные обновления.
3. **Wi-Fi 4-address/WDS uplink, Ethernet fallback.** Wi-Fi остаётся на PVE,
   `vmbr0` сохраняет L2; VM IP/URL не меняются. NAT/PCI passthrough не нужны.
   Оба uplink одновременно в мост не подключать.
4. **Собственный VPN + приватные HTTPS-имена, не публичный HA.** Наружу
   проброшен только `443` на ocserv (сертификат клиента или пароль + TOTP).
   Сервисы — Traefik в K3s с wildcard Let's Encrypt (Cloudflare DNS-01) под
   приватным суффиксом, allow-list LAN; DNS-записи указывают на LAN-адрес.
5. **STT/TTS локально.** GigaAM primary, Whisper fallback, Piper Puriso;
   Hermes — только свободная речь. Облако видит текст, но не аудио.
6. **Hermes не владеет state.** Будильники/таймеры/напоминания — scheduler
   (решение 10); громкость/музыка — HA; локальные intents работают без LLM.
7. **MA + Spotify Connect.** MA — локальный хаб/плейлисты; Connect — телефон;
   Pulsar — Sendspin-плеер. Один аккаунт = один stream-slot.
8. **BLE gateway — аварийный provisioning.** Работает без Wi-Fi/облака;
   GATT SSID/PSK/token; 1/3/4/5. Старый QR/DPP провижининг удалён.
9. **OTA/ADB fortress + egress guard.** Защита от OTA/сброса/закрытия ADB;
   публичный WAN и Яндекс закрыты. Factory reset сносит защиту.
10. **Расписания — canonical scheduler, звук — HA.** `solutions/scheduler`
    владеет будильниками/таймерами/напоминаниями; `ha-bridge` исполняет
    очередь `home` (alarm_start/announce/alarm_stop), голосовые intents и
    Android работают через Connect API. Legacy HA-тики отключены.
11. **Recovery — локально + offsite.** HA/K3s/PVE/станция; проверять SHA-256.
    Factory-образа Midi нет; OTA-блок не заменяет recovery.
12. **Монорепозиторий + Bazel.** Go/Rust/mobile; hermetic build; тесты рядом
    с кодом. Rust — DSP/инференс, Go — control plane.
13. **Home-сценарии — метка HA `scenario`.** Скрипт попадает в каталог Home
    только с этой нативной меткой; иконки/комнаты/имена — отображение.
14. **Reminder-грамматика — один слот `{text}`.** Фраза поддерживает один
    текстовый фрагмент до или после даты/времени; два фрагмента вокруг даты
    осознанно не распознаются. Расширение — отдельное решение.
15. **SSH станции — ключ, пароль как fallback.** Ключ Mac
    `~/.ssh/ricardo_station_ed25519` в `/root/.ssh/authorized_keys`; вход по
    паролю осознанно оставлен аварийным. ADB/SSH — только LAN.
16. **Legacy HA-helper-ы расписаний удалены.** `todo.alarms_*` и
    `todo.reminders` удалены, `timer.taimer_1…8` отключены в registry;
    источник истины — scheduler. Заметки остаются в `todo.notes`.
17. **Сервисы K3s — digest-пины образов, деплой только kubectl.** scheduler,
    ha-bridge и ha-control собираются Bazel `rules_oci` (distroless,
    digest-pinned base) и адресуются `ghcr.io/proflyder/<name>@sha256:...`;
    `imagePullPolicy: IfNotPresent`. CI пушит образы job-scoped
    `GITHUB_TOKEN` и деплоит kubectl под ServiceAccount `ci-deploy` через
    VPN `tcp/6443`. SSH, root-helper, hostPath и `/opt/ricardo` из пути
    деплоя удалены; NodePort-nft остаётся host-конфигурацией.

18. **OpenBao — единственный секрет-сервер.** OpenBao 2.7 (chart и образ
    по digest) в namespace `openbao`, UI/API — `vault` под приватным
    суффиксом через Traefik и общий allow-list. Raft на локальном Retain-PV,
    audit — файл на отдельном PV. Auto-unseal — встроенный `static` seal:
    ключ в Secret на том же узле, чтобы перезагрузка не требовала человека
    (компромисс до внешнего KMS; ключ не входит в snapshot). Человек —
    `userpass` + TOTP, политика `admin` без `sys/seal`/`raw`/`rekey`; CI —
    GitHub OIDC (repository, environment, ref `develop`, TTL ≤ 15 мин, только
    `secret/ci/*`); поды — Kubernetes auth (только `secret/k8s/*`). Root-токен
    отозван, recovery keys — только офлайн.

Меняется решение — обновить этот файл и `STATE.md`.
