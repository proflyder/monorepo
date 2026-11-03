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
23. **Внешний dead-man switch — Cloudflare Worker и тот же инфра-бот.**
    Alertmanager маршрутизирует всегда горящий `Watchdog` на
    `solutions/heartbeat` (Cloudflare Worker, KV, Cron `*/5`). Тишина дольше
    grace-периода и восстановление связи уходят в Telegram через существующего
    инфра-бота, без нового бота и стороннего мониторинга. Компромисс:
    тот же `TELEGRAM_BOT_TOKEN` инфра-бота (полные права бота) хранится ещё и
    в Cloudflare Worker secrets вне дома; при ротации бота обновлять оба места;
    `PING_TOKEN` ограничивает приём heartbeat и хранится также в OpenBao
    `secret/k8s/monitoring/heartbeat`. Worker остаётся reusable-решением,
    project-specific URL/ids — только в private env проекта.

24. **Offsite backup — Google Drive + restic, ключи отдельно.** Личный
    Google Drive (15 GB, без привязки карты: перерасход невозможен) через
    rclone с scope `drive.file`; Cloudflare R2 отклонён — у него нет
    жёсткого лимита расходов. Restic шифрует данные на K3s-узле до отправки;
    скрипт не загружает > 2 GiB за запуск и падает при репозитории > 5 GiB. В offsite-копию входят только
    восстановимые данные: raft snapshot OpenBao, online-копии SQLite
    scheduler/currency-bot, ledger ha-bridge и уже зашифрованный
    автоматический backup HA. Не вывозятся: `secrets/`, CA, VPN-ключи,
    OpenBao seal/recovery keys — без них offsite-копия не должна давать
    полный доступ. Скрипт явно отказывает файлам `*seal*`/`*recovery*`/`*.key`.
    restic password хранится в password manager вне дома (Google-доступ
    восстанавливается новым OAuth);
    при потере любого из них копия бесполезна. Возраст источников и успех
    запуска публикуются в node-exporter textfile и проверяются алертами;
    PVE scrape target и rclone token — только private values/env, не Git.

25. **Голосовой профиль — ручной режим, а не догадка модели.** Источник
    истины — `input_select.ricardo_voice_profile` в HA (Личный/Рабочий);
    меняется только явно: голосом, дашбордом или `switch.ricardo_work_profile`
    из Android (нативная HA-метка `voice_profile` → каталог
    `kind: "profile"`). Профиль не выводится из текста фразы, времени или
    приложения и не переключается автоматически; автовозврата нет. Влияет
    только на свободную речь: локальные бытовые intents работают в обоих
    профилях. Ошибка чтения профиля — отказ («Не могу подтвердить профиль»),
    а не догадка.

26. **Hermes — только личный профиль.** Голос, Telegram и `bb_status`/
    `bb_prepare_spawn` видят лишь проекты профиля `personal`; рабочие проекты
    и проекты с неизвестным профилем отфильтрованы шлюзом и неотличимы от
    несуществующих (без `profile_mismatch` и без имён). Запуск агента всегда
    двухшаговый (`prepare` → подтверждение голосом/в Telegram → `confirm`,
    pending TTL 90 с); свободный текст вместо имени проекта не принимается.
    «По работе» решается переключением профиля колонки, а не проектом в
    Hermes.

27. **Рабочая речь не уходит в облачную LLM, кроме рабочего Claude.** В
    рабочем профиле голосовой путь идёт только в постоянный тред-диспетчер
    Claude Code на рабочем Mac. Ошибка bb, недоступный шлюз или
    ненастроенный тред остаются на маршруте `bb` с короткой фразой; откат в
    Hermes запрещён (fail-closed). Личный профиль по-прежнему использует
    Hermes как раньше.

28. **bb доступен домашним сервисам только через плагин и SSH forced
    command.** API bb наружу не открывается: HTTP-маршрут плагина
    `home-dispatcher` слушает loopback на CT 103 и требует токен плагина +
    HMAC; снаружи в него ходит только `mcp-gateway` по SSH с отдельным
    ограниченным ключом, pinned `known_hosts` и forced command
    `bb-dispatch`. Токен плагина и HMAC-секрет живут только на CT; шлюз
    знает только свой ключ, не имеет shell и не может вызвать произвольный
    метод. Маршрут плагина через bb Connect закрыт (401).

29. **Встроенный MCP bb (`bb--mcp`) не используется для голоса.** Он
    отдаёт клиенту весь набор инструментов bb сразу (all-or-nothing):
    нельзя ограничить группы, проекты и профиль, нельзя отделить голосовой
    токен и получить короткий озвучиваемый отказ. Поэтому голосовой путь
    идёт через собственный тонкий плагин с шестью методами и явным
    контрактом полей, а не через универсальный MCP-сервер bb.

30. **Свой credential provider вместо Windows autologon.** Автовход
    отклонён владельцем: он входил бы и после включения кнопкой, а прошивка
    не отличает Wake-on-LAN от нажатия кнопки. Решение о входе принимает HA
    (`switch.komp.turn_on` выдаёт одноразовый грант до WoL), поэтому на
    экране входа появляется отдельная плитка только при действующем гранте;
    без него остаётся обычный вход по PIN. Провайдер —
    `solutions/windows-logon-provider`, CLSID
    `{9459CCEA-4CCC-4435-89BF-E0E132E0C27F}`.

31. **Грант принимается только в `CPUS_LOGON`, никогда при разблокировке.**
    `CPUS_UNLOCK_WORKSTATION` не запускает сеть, не показывает плитку и
    возвращает `GetCredentialCount` = 0. Иначе одноразовый пропуск мог бы
    разблокировать уже работающий ПК; грант существует только для холодного
    входа после Wake-on-LAN. Остальные сценарии — `E_NOTIMPL`.

32. **`wakegrant` — отдельный маленький сервис, не часть `mcp-gateway`.**
    Он принимает plain HTTP только в LAN и защищён HMAC в обе стороны поверх
    серверного nonce: секрет не передаётся по сети, неверный MAC/MAC без
    nonce — отказ (fail-closed). Грант одноразовый, живёт 180 с и хранится в
    SQLite на PVC; `mcp-gateway` остаётся инфраструктурным шлюзом и не
    владеет пропусками входа. Контракт —
    `packages/contracts/wakegrant/PROTOCOL.md`.

33. **Windows-компонент собирается llvm-mingw без Visual Studio.**
    Провайдер и self-tests — C++17, кросс-сборка pinned llvm-mingw
    (`20261020`) с macOS и Linux CI, переносимые проверки — host clang++.
    Код основан на MIT-примерах Microsoft
    (`Windows-classic-samples`), GPL/unlicensed-код не используется;
    уведомления — `THIRD-PARTY-NOTICES.md`. CI собирает DLL, self-tests и
    `SHA256SUMS` и не трогает ПК (он недоступен из CI); распространение —
    решение 34.

34. **Windows-бинарники — публичные OCI-артефакты в GHCR через ORAS.**
    Провайдер входа — отдельный пакет
    `ghcr.io/proflyder/windows-logon-provider` с семантической версией из
    `VERSION` и тегом `sha-<12 hex>`. Тег версии неизменяем по содержимому:
    тот же `SHA256SUMS` — no-op, другой — ошибка, поэтому релиз — смена
    `VERSION`. Установка — по digest (`@sha256:…`) после `sha256sum -c` и
    `gh attestation verify`; build provenance создаётся для digest артефакта
    (`push-to-registry`) и для DLL. Публикует только push в `develop`
    (`deploy-home`), переиспользуя сборку `_native.yml`; PR никогда не
    публикует. GitHub Releases отклонены (монорепозиторий смешал бы продукты
    в одном потоке релизов), Maven/NuGet — тоже (даже публичное чтение
    требует авторизации). Новый пакет GHCR может создаться приватным:
    публичность включается один раз в GitHub UI (Package settings → Change
    visibility → Public), после чего `oras pull` анонимный.

35. **Язык выбирается по месту исполнения, а не по типу задачи.**
    * Всё, что исполняется в k3s (Deployment, CronJob, Job, init/sidecar),
      и инструменты деплоя, которые собирают, рендерят и сходят в кластер,
      пишется на Go: общий конвейер Bazel → OCI-образ → `deploy-home`,
      общие пакеты, один статический бинарник, нет второго базового образа.
    * Задачи на хостах (Proxmox, LXC, ПК): короткая обвязка вызовов команд
      (ориентир до ~200 строк, без протоколов и криптографии) — shell или
      Python stdlib; большая логика (свой HTTP/API-клиент, шифрование,
      проверки целостности) — Go-бинарник из CI, доставляемый по digest
      (как в решении 34).
    * CI-обвязка GitHub Actions — JavaScript через официальный
      `actions/github-script` (Node уже есть в раннере, без setup-*/pip);
      логика длиннее нескольких строк — модули в `platform/ci/` и
      `platform/quality/` с `node --test`, без npm-зависимостей; локальный
      запуск — `node`.
    * Python остаётся для ML/обучения wake-word, dev-утилит станции,
      проверок YAML-конфигов и live-e2e, где он не исполняется в проде.
    Существующий код переводится по этому правилу поэтапно, отдельными PR;
    рабочее поведение и тестовое покрытие при переводе сохраняются.

Меняется решение — обновить этот файл и `STATE.md`.
