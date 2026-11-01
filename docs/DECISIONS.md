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
   проброшен только `443/tcp+udp` на ocserv (сертификат клиента или пароль +
   TOTP). Сервисы — Traefik в K3s с wildcard Let's Encrypt (Cloudflare
   DNS-01) под приватным суффиксом, allow-list LAN/VPN; DNS-записи указывают
   на LAN-адрес.
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
    `GITHUB_TOKEN`, а `deploy-home` на push в `develop` катит только
    затронутые юниты через VPN группу `ci` и kubectl под ServiceAccount
    `ci-deploy`. SSH, root-helper, hostPath и `/opt/ricardo` из пути
    деплоя удалены; NodePort-nft остаётся host-конфигурацией.

18. **OpenBao — единственный секрет-сервер.** OpenBao 2.7 (chart и образ
    по digest) в namespace `openbao`, UI/API — `vault` под приватным
    суффиксом через Traefik и общий allow-list. Raft на локальном Retain-PV,
    audit — в stdout → Loki (заполненный диск
    не блокирует запросы). Auto-unseal — встроенный `static` seal:
    ключ в Secret на том же узле, чтобы перезагрузка не требовала человека
    (осознанно принятый риск, решение владельца 2026-11-01: внешний
    KMS/HSM/TPM не вводим; ключ не входит в snapshot и в offsite-бэкап
    кладётся отдельно от raft-снапшотов). Человек —
    `userpass` + TOTP, политика `admin` без `sys/seal`/`raw`/`rekey`; CI —
    GitHub OIDC (repository, environment, ref `develop`, TTL ≤ 15 мин, только
    `secret/ci/{test,deploy-home}`); поды — Kubernetes auth (только
    `secret/k8s/*`). Root-токен отозван, recovery keys — только офлайн.

19. **Git — только PR, squash и `ci-ok`.** Прямой push в `develop` запрещён;
    merge PR запускает `deploy-home`. Режимы исполнения меняются только
    ручным `workflow_dispatch`; откат — только применённого объекта, полный —
    новым revert-PR.
20. **Pulsar — релизы в форке, деплой через HA updater.** Монорепо Pulsar не
    собирает: сборка и GitHub release выходят из форка (origin вложенного
    checkout `projects/station/upstream/pulsar-rust`, линия `main` = Ricardo),
    теги `v<major.minor>.<1000+N>` — updater ставит только строго большую
    версию. Выкатка на станцию — только `deploy-station.yml` после ручного
    approve в Environment `Station` (теги `v*`), через VPN группу `ci` и OIDC
    в OpenBao (`deploy-station`), затем HA `update.install`; job проверяет
    установленную версию и стабильность. Откат — не через CI, а на станции
    (`.prev` + `ctl.sh reload`). Egress станции при этом разрешает HTTPS
    только к публичным IPv4-диапазонам GitHub, весь остальной WAN и Яндекс —
    DROP.

21. **Currency bot — Go-переписывание, Cloudflare Tunnel, OpenBao вместо
    Unkey.** Kotlin `tg-bot-currency` и VPS-стек `infra-monitoring`/`infra-ci`
    заменены монорепо, старые репозитории архивируются. Поведение 1:1
    (kurs.kz, часовой job, алерты HOUR/DAY/WEEK/MONTH с dedup, retention
    35 дней), но SQLite вместо H2, webhook на отдельном порту `8081` с
    Telegram `secret_token` и allowlist чатов, Bearer-токен API из OpenBao
    вместо Unkey. Публичный ingress — только
    `tg.example.com/telegram/webhook` через Cloudflare Tunnel `home-k3s`;
    API остаётся LAN/VPN. Логи — JSON в stdout, метрики — Prometheus,
    деплой — data-driven CI-юнит `currency-bot`.

22. **Один MCP-шлюз, перезапуск только с подтверждением.** Вся
    инфраструктурная диагностика идёт через `solutions/mcp-gateway`: Hermes
    (голос и Telegram) не имеет ServiceAccount-токена и kubectl, доступ к
    Prometheus/Alertmanager/Loki/Kubernetes API есть только у шлюза. Группы
    `infra.read` (чтение), `infra.fix` (единственная мутация — rollout restart
    Deployment/StatefulSet/DaemonSet из каталога, два шага: prepare → confirm,
    TTL 90 с, cooldown 5 мин) и `infra.report` (Telegram, получатель
    фиксирован). Перезапуск разрешён для всех repo-managed workload'ов только
    после голосового или Telegram-подтверждения, у каждого — честное
    предупреждение; `hermes`, `voice-proxy` (Deployment `hermes`) и
    `mcp-gateway` помечены `self_disrupting`, патч откладывается ~20 с, чтобы
    ответ успели озвучить. Внешние HAOS/MA/станция — только чтение. Список
    целей — каталог (ConfigMap), и RBAC `patch` с `resourceNames` обязан
    совпадать с ним (contract-тест). Логи — Loki там, где Alloy шипит
    namespace, иначе `pods/log`; отчёты и алерты — отдельный инфра-бот,
    бот currency-bot остаётся только у currency-bot.

Меняется решение — обновить этот файл и `STATE.md`.
