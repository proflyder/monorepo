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
20. **Pulsar — код и релизы в монорепозитории, деплой через HA updater.**
    Исходники — `projects/station/pulsar/`; CI — юнит `pulsar`
    (`_native.yml`: Bazel-сборка обеих архитектур, installer, radio, ssh), хост
    fmt/clippy/тесты — отдельный юнит `pulsar-bazel` (`_bazel.yml`).
    Версия — `projects/station/pulsar/VERSION` (`v<major.minor>.<1000+N>`).
    Pulsar — обычный publishable native-юнит общего `deploy-home`: поле
    `native.publish_when` пускает его в `native_units` на push только при
    изменении VERSION; прочие изменения Pulsar — только PR CI. `native-release`
    продвигает проверенный PR-артефакт `tree-<hash>` на
    `ghcr.io/proflyder/pulsar-firmware:<version>` (+ `:latest`) или при
    missing/unverified собирает и публикует заново (решение 36); GitHub
    Releases и теги в монорепо не используются.
    Выкатка на станцию — job `deploy-station` после ручного approve в
    Environment `Station`, через VPN группу `ci` и OIDC в OpenBao
    (`deploy-station`), затем HA `update.install`; job проверяет
    установленную версию и стабильность. `workflow_dispatch` с
    `units=pulsar` повторно катит уже опубликованный VERSION, не пересобирая
    и не перезаписывая тег. Откат — не через CI, а на станции (`.prev` +
    `ctl.sh reload`). Egress станции при этом разрешает HTTPS только к
    публичным IPv4-диапазонам GitHub, весь остальной WAN и Яндекс — DROP.

21. **Currency bot — Go-переписывание, Cloudflare Tunnel, OpenBao вместо
    Unkey.** Заменяет Kotlin `tg-bot-currency` и VPS-стек. Поведение
    прежнего бота сохранено
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
    (`push-to-registry`) и для DLL. PR из этого репозитория публикует
    release-shaped артефакт под `tree-<hash>` (build once), а push в
    `develop` продвигает проверенный digest на теги `VERSION` и `sha-<12>`
    либо пересобирает при missing/unverified (решение 36); форки не
    публикуют. GitHub Releases отклонены (монорепозиторий смешал бы продукты
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
    * Инструменты, которые запускает сама сборка Bazel (genrule, правила,
      тестовые раннеры), — Go-бинарники `go_binary` в exec-конфигурации:
      сборка герметична и не зависит от `python3` хоста.
    * Python остаётся для ML/обучения wake-word, dev-утилит станции,
      проверок YAML-конфигов (включая отчёт `make coverage`) и live-e2e, где
      он не исполняется в проде.
    Существующие
    хостовые и операторские скрипты на Python stdlib — `offsite_backup.py`,
    `vpncerts.py`/`vpnusers.py`, `openbao/configure.py`/`restore_drill.py` —
    оставлены сознательно: они запускаются вручную или по таймеру, не стоят
    на пути деплоя, покрыты тестами и не требуют pip. Их перевод на Go —
    только в составе существенной переделки самого скрипта (новый бэкенд
    хранения, PKI в OpenBao и т.п.), а не отдельной задачей; новые скрипты
    такого объёма сразу пишутся по правилу выше.

36. **Артефакт собирается один раз на PR; деплой — по git tree hash и provenance.**
    * Идентичность собранного артефакта — git tree собираемого коммита. PR
      собирает merge-ref, и его `git rev-parse HEAD^{tree}` совпадает с tree
      squash-коммита, пока `develop` не ушёл вперёд. Поэтому тег —
      `tree-<полный tree hash>`, а не sha коммита. Правило одно для
      контейнерных образов и native-артефактов (Pulsar, Windows logon
      provider).
    * `ci.yml` → `_images.yml` для PR из этого репозитория (форки — только
      сборка) после сборки публикует каждый образ затронутых деплой-юнитов в
      `ghcr.io/proflyder/<image>:tree-<hash>` и создаёт build provenance для
      digest официальным `actions/attest-build-provenance`
      (`push-to-registry`). Bazel-образы пушатся существующими `<image>_push`
      побайтно (digest после push обязан совпасть со собранным), Docker-образы
      — `docker push`. Права `packages: write`, `id-token: write`,
      `attestations: write` выданы только job `images` в `ci.yml`.
    * `ci.yml` → `_native.yml` делает то же для native-юнитов с
      `native.publish_script`: после сборки публикует release-shaped
      OCI-артефакт в `native.publish_repository` под `tree-<hash>` и аттестует
      тот же digest. Pulsar собирается release-shaped на каждом PR, а не только
      при изменении VERSION: бинарники обеих архитектур с версией из
      `projects/station/pulsar/VERSION` (`--define`), radio `.ybl`,
      `SHA256SUMS`, те же заголовки слоёв/тип/аннотации; `--version`
      проверяется под qemu. Тег версии и `latest` ставит только deploy-home.
      Logon provider публикует свой обычный артефакт. `publish-oci.sh` умеет
      `--tag` (пуш ровно под одним тегом) и обычный режим релизных тегов.
      Права выданы только job `native` в `ci.yml`; форки только собирают.
    * `deploy-home` считает tree деплоимого коммита. Для образов digest из
      `tree-<hash>` используется, только если тег существует, а provenance
      проверяется `gh attestation verify oci://… --repo <owner>/<repo>` с
      signer, ограниченным `ci.yml`/`_images.yml` этого репозитория. Обычные
      теги (`<commit sha>`, `develop`) ставятся на тот же digest pinned-`crane`,
      digest передаётся существующему deploy-скрипту через
      `PREBUILT_DIGESTS`; скрипт пропускает сборку/push только для этих
      образов. Любой отсутствующий или непроверенный образ деплой-юнита —
      откат на обычную сборку; выбранный путь пишется в job summary. Неверный
      tree не может быть задеплоен.
    * Native-юнит `deploy-home` продвигает через `platform/ci/promote.mjs
      --native`: требует существующий `tree-<hash>`, проверяет provenance с
      signer `ci.yml`/`_native.yml` и ставит релизные теги из
      `native.publish_tags` (`{version}`, `latest`, `sha-{commit12}`) на тот же
      digest pinned-`crane`. Неизменяемые теги (`<version>`, `sha-<commit>`)
      неизменяемы по СОДЕРЖИМОМУ: `crane manifest` сравнивает `artifactType` и
      упорядоченный список слоёв `(title, digest, mediaType)`; совпадающий
      контент при другом digest манифеста (различаются только аннотации
      `created`/`revision`) — no-op без перетегирования с пометкой «already
      published with identical content», другой контент — отказ «bump
      VERSION». `latest` двигается всегда, но на digest тега версии: если
      версия сохранена, `latest` указывает на сохранённый digest, поэтому
      `latest` и `<version>` согласованы, а `deploy-station` ставит именно
      `<version>`. При отсутствии/непроверенном tree — обычная сборка и
      публикация, путь `build` с пометкой fallback в summary. `deploy-station`
      ставит версию и проверяет digest'ы слоёв.
    * Любой job, который пишет в GHCR (`crane tag`/`crane push`, `oras
      push`/`oras tag`, `promote.mjs`, `push-to-registry`), логинится в
      `ghcr.io` до первой записи. Публичные пакеты читаются анонимно, поэтому
      без логина падает только PUT манифеста с `UNAUTHORIZED`.
      `check_units.mjs` проверяет это по workflow-файлам.
    * `ghcr-cleanup.yml` (ежедневно по расписанию; ручной запуск — dry-run по
      умолчанию) через REST API применяет политику хранения по пакету:
      никогда не удаляются новейшая версия, версии с тегами `develop`/`latest`
      и semver, версии с `sha-<commit>` (native-релизы), digest'ы из явного
      `PROTECTED_DIGESTS` в модуле и версии моложе 7 дней. Deploy-версии (есть
      40-hex commit-sha тег) сохраняют пять новейших; более старые удаляются,
      если им больше 7 дней. Устаревшие версии только с `tree-*` и без тегов
      удаляются. Referrer-версии (`sha256-<hex>` теги, cosign-style) удаляются
      вместе с digest'ом, на который указывают, либо если он уже отсутствует в
      пакете; в окно пяти deploy-версий они не входят. `PROTECTED_DIGESTS`
      сейчас пуст (exporter Hermes переведён на Go-образ `health-exporter`).
      Пакеты — образы деплой-юнитов и `native.publish_repository` из
      `units.mjs`; прочие пакеты аккаунта не рассматриваются. Для
      `pulsar-firmware` и `windows-logon-provider` это означает: релизные теги
      (`latest`, `vX.Y.Z`/`X.Y.Z`, `sha-<12>`) сохраняются, устаревшие
      `tree-*`-only удаляются через 7 дней.

37. **Pulsar — first-party код в монорепозитории.** Релиз, публикация и
    выкатка — решение 20.
    * `projects/station/pulsar/` — AGPL-3.0 (`LICENSE`, README, docs,
      RICARDO.md сохранены). Upstream `Morvex885/pulsar` вливается вручную
      патчем: `git diff vA..vB` → `git apply -3 --directory=projects/station/pulsar`,
      затем обычный squash PR; автоматического fork-sync нет.
    * GitHub Releases и теги в монорепо запрещены (pre-push hook).
      Существующий тег версии в GHCR с другим digest перезаписать нельзя —
      выпускается следующая версия; тот же digest — no-op.
    * `proflyder/pulsar-firmware` — публичный пакет, созданный workflow из
      `GITHUB_TOKEN` (`packages: write`); имя зашито в
      `pulsar_update::UPDATE_REPO` установленной прошивки. `ghcr-cleanup`:
      `latest` и версии защищены, устаревшие `tree-*`-only удаляются через
      7 дней.

38. **Pod template аннотируется содержимым, а не git-ревизией.**
    * Почему: git-ревизия в pod template перезапускала все pods на каждом
      merge, даже при тех же образах и конфиге.
    * Pod template несёт `ricardo.dev/rollout`, а
      `home-deploy render --rollout-key ROLLOUT_ID` заполняет его первыми
      12 hex sha256 **всего отрендеренного юнита** (digest'ы всех
      контейнеров, значения из private env, содержимое ConfigMap, probes,
      resources). При хешировании маскируются `BUILD_ID` (git-ревизия) и сам
      `ROLLOUT_ID`, поэтому хеш зависит только от того, что исполняют pods.
    * `BUILD_ID` остаётся аннотацией самого Deployment
      (`metadata.annotations.ricardo.dev/build`) для трассировки; изменение
      metadata не меняет generation и не катит pods. `converge
      --summary-file` пишет `unchanged|rolled-out` по сравнению
      `metadata.generation` до и после apply; `deploy-home` показывает это в
      job summary.
    * Изменение образа, отрендеренного конфига или смонтированного
      ConfigMap по-прежнему перезапускает pods. ESO-managed Secret'ы не
      входят в хеш: они не рендерятся юнитом.
    * Golden-фикстуры и `rollout_test.go` фиксируют: другая git-ревизия при
      тех же образах/конфиге → идентичный pod template; другой digest или
      конфиг → другой.

Меняется решение — обновить этот файл и `STATE.md`.
