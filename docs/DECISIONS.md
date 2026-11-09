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
12. **Монорепозиторий + Bazel.** Rust/Kotlin/Python/JS; hermetic build; тесты рядом
    с кодом. Языки рантайма — решение 41.
13. **Home-сценарии — метка HA `scenario`.** Скрипт попадает в каталог Home
    только с этой нативной меткой; иконки/комнаты/имена — отображение.
14. **Reminder-грамматика — один слот `{text}`.** Фраза поддерживает один
    текстовый фрагмент до или после даты/времени; два фрагмента вокруг даты
    осознанно не распознаются. Расширение — отдельное решение.
15. **SSH станции — ключ, пароль как fallback.** Ключ Mac
    `~/.ssh/home-lab/station_ed25519` в `/root/.ssh/authorized_keys`; вход по
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
    `ci-deploy`. SSH, root-helper, hostPath и `/opt/home` из пути
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
    fmt/clippy/тесты — отдельный юнит `pulsar-bazel` (`_unit.yml`).
    Версия — `projects/station/pulsar/VERSION` (`v<major.minor>.<1000+N>`).
    Pulsar — обычный publishable native-юнит общего `deploy-home`: поле
    `native.publish_when` пускает его в `native_units` на push только при
    изменении VERSION; прочие изменения Pulsar — только PR CI. `native-release`
    продвигает проверенный PR-артефакт `tree-<hash>` на
    `ghcr.io/proflyder/pulsar-firmware:<version>` (+ `:latest`); при
    missing/unverified деплой падает и ничего не собирает (решение 36).
    GitHub Releases и теги в монорепо не используются.
    Выкатка на станцию — job `deploy-station` после ручного approve в
    Environment `Station`, через VPN группу `ci` и OIDC в OpenBao
    (`deploy-station`), затем HA `update.install`; job проверяет
    установленную версию и стабильность. `workflow_dispatch` с
    `units=pulsar` повторно катит уже опубликованный VERSION, не пересобирая
    и не перезаписывая тег. Откат — не через CI, а на станции (`.prev` +
    `ctl.sh reload`). Egress станции при этом разрешает HTTPS только к
    публичным IPv4-диапазонам GitHub, весь остальной WAN и Яндекс — DROP.

21. **Currency bot — Rust-сервис, Cloudflare Tunnel, OpenBao вместо
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
    истины — `input_select.home_voice_profile` в HA (Личный/Рабочий);
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

31. **Грант принимается только по своему назначению: `logon` — после
    холодной загрузки, `unlock` — для заблокированной сессии того же
    пользователя.** Режим выбирает чистая функция по сценарию, WTS-сессии
    настроенного пользователя (любое состояние; ошибка запроса — отказ) и
    аптайму: сессия есть → `unlock` при `CPUS_LOGON` или
    `CPUS_UNLOCK_WORKSTATION`, сессии нет и аптайм меньше 10 минут → `logon`,
    иначе `off` (Windows 11 вызывает `CPUS_LOGON` и при блокировке).
    Unlock-воркер шлёт `purpose=unlock` с long-poll `waitSeconds=25` без
    бюджета, logon-воркер — `purpose=logon` с бюджетом 90 с; MAC обоих
    направлений включает purpose, поэтому одно назначение не может
    выполнить действие другого. Остальные сценарии — `E_NOTIMPL`.

32. **`wakegrant` — отдельный маленький сервис, не часть `mcp-gateway`.**
    Он принимает plain HTTP только в LAN и защищён HMAC в обе стороны поверх
    серверного nonce: секрет не передаётся по сети, неверный MAC/MAC без
    nonce — отказ (fail-closed). Грант одноразовый, живёт 180 с и хранится в
    SQLite на PVC; `mcp-gateway` остаётся инфраструктурным шлюзом и не
    владеет пропусками входа. Контракт —
    `packages/contracts/wakegrant/PROTOCOL.md`.

33. **Windows-компонент (C++/mingw) — отменено решением 39.**

34. **Windows-бинарники — публичные OCI-артефакты в GHCR через ORAS.**
    Провайдер входа — отдельный пакет
    `ghcr.io/proflyder/windows-logon-provider` с семантической версией из
    `VERSION` и тегом `sha-<12 hex>`. Тег версии неизменяем по содержимому:
    тот же `SHA256SUMS` — no-op, другой — ошибка, поэтому релиз — смена
    `VERSION`. Установка — по digest (`@sha256:…`) после `sha256sum -c` и
    `gh attestation verify`; build provenance создаётся для digest артефакта
    (`push-to-registry`) и для DLL. PR из этого репозитория публикует
    release-shaped артефакт под `tree-<hash>` (build once), а push в
    `develop` продвигает проверенный digest на теги `VERSION` и `sha-<12>`;
    при missing/unverified деплой падает, ничего не собирая (решение 36);
    форки не публикуют. GitHub Releases отклонены (монорепозиторий смешал бы продукты
    в одном потоке релизов), Maven/NuGet — тоже (даже публичное чтение
    требует авторизации). Новый пакет GHCR может создаться приватным:
    публичность включается один раз в GitHub UI (Package settings → Change
    visibility → Public), после чего `oras pull` анонимный.

35. **Язык выбирается по месту исполнения, а не по типу задачи.**
    * Долгоживущие сервисы в k3s и инструменты деплоя — Rust (решения 41
      и 35): общий конвейер Bazel → OCI-образ → `deploy-home`, общие пакеты,
      один статический бинарник, нет второго базового образа.
    * Задачи на хостах (Proxmox, LXC, ПК): короткая обвязка вызовов команд
      (ориентир до ~200 строк, без протоколов и криптографии) — shell или
      Python stdlib; большая логика (свой HTTP/API-клиент, шифрование,
      проверки целостности) — Rust-бинарник из CI, доставляемый по digest
      (как в решении 34).
    * CI-обвязка GitHub Actions — JavaScript через официальный
      `actions/github-script` (Node уже есть в раннере, без setup-*/pip);
      логика длиннее нескольких строк — модули в `platform/ci/` и
      `platform/quality/` с `node --test`, без npm-зависимостей; локальный
      запуск — `node`.
    * Инструменты, которые запускает сама сборка Bazel (genrule, правила,
      тестовые раннеры), — на языке соответствующего стека в
      exec-конфигурации: Android — Kotlin `kt_jvm_binary`/`kt_jvm_test`
      (`platform/android/tools`), серверные и деплойные — Rust/Bazel.
      Сборка герметична и не зависит от `python3` хоста.
    * Python остаётся для ML/обучения wake-word, dev-утилит станции,
      проверок YAML-конфигов (включая отчёт `mise run coverage`) и live-e2e, где
      он не исполняется в проде.
    * Итог: Go в репозитории нет. Деплой-инструменты — Rust
      (`platform/deploy-rs`), серверные Bazel-инструменты — Rust, Android —
      Kotlin. Языки репозитория: Rust, Kotlin, Python, JS/TS.
    Существующие
    хостовые и операторские скрипты на Python stdlib — `offsite_backup.py`,
    `vpncerts.py`/`vpnusers.py`, `openbao/configure.py`/`restore_drill.py` —
    оставлены сознательно: они запускаются вручную или по таймеру, не стоят
    на пути деплоя, покрыты тестами и не требуют pip. Их перевод на Rust —
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
    * `ci.yml` → `_unit.yml` собирает проверки юнита и его образы в одном job
      и одном Bazel-сервере: Rust-сервис больше не строится дважды в
      `_bazel`/`_images`. Для PR из этого репозитория (форки — только сборка)
      каждый образ публикуется в `ghcr.io/proflyder/<image>:tree-<hash>` и
      аттестуется официальным `actions/attest-build-provenance`
      (`push-to-registry`). Bazel-образы пушатся существующими `<image>_push`
      побайтно (digest после push обязан совпасть со собранным), Docker-образы
      — `docker push`. Attestation — статические шаги того же job'а (по
      одному на индекс образа; action аттестует один OCI-subject). Права
      `packages: write`, `id-token: write`, `attestations: write` выданы
      только job `unit` в `ci.yml`.
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
      signer, ограниченным `ci.yml`/`_unit.yml` этого репозитория (плюс
      исторические `_images.yml`/`_attest.yml`). Обычные
      теги (`<commit sha>`, `develop`) ставятся на тот же digest pinned-`crane`,
      digest передаётся существующему deploy-скрипту через
      `PREBUILT_DIGESTS`. **`deploy-home` ничего не собирает:** любой
      отсутствующий или непроверенный образ деплой-юнита — ошибка job'а с
      причиной в summary, а не откат на сборку; ни `bazel build`/`bazel run`,
      ни `docker build` на деплой-раннере не выполняются (`DEPLOY_NO_BUILD=1`).
      Сборка тех же образов локально/вручную остаётся доступной, если явно не
      выставлен `DEPLOY_NO_BUILD`.
    * `_unit.yml` для каждого затронутого деплой-юнита дополнительно
      публикует образ `ghcr.io/proflyder/home-deploy:tree-<hash>` (если юнит
      не перечисляет его сам) и аттестует его digest. `deploy-home` достаёт
      бинарник CLI из промоутнутого образа (`crane export`, путь
      `/usr/local/bin/home-deploy`) и передаёт его скриптам как `HOME_DEPLOY`.
      Механизм не зависит от языка: образ собирается из Rust-крейта
      `platform/deploy-rs:home-deploy`.
    * Ручной `workflow_dispatch` выкатывает текущий `develop`, у которого нет
      CI-сборки для юнитов, не затронутых последним merged PR (tree меняется
      на каждом merge). Поэтому promotion для каждого образа, у которого нет
      верифицированного `tree-<hash>` текущего коммита, идёт по first-parent
      истории (~200 коммитов) деплоимого коммита и берёт самый свежий коммит,
      у которого тег `tree-<tree(commit)>` существует и provenance
      проверяется. Корректность: CI собирает образ при любом изменении его
      входов, поэтому изменение после найденного коммита дало бы более
      поздний — и найденный первым — tree. Коммит-источник пишется в summary.
      На push это правило действует только для образов-соседей, которые
      скрипт рендерит, но не применяет (`deploy.optional_images` в
      `units.mjs`: ha-bridge/ha-control при деплое scheduler и наоборот);
      собственные образы юнита на push обязаны совпадать с текущим tree
      либо приходить из PR head (см. следующий пункт), иначе job падает.
      Соседние образы не перетегируются — их digest только
      передаётся рендеру. Зависимости от состояния кластера и от `develop`-тега
      нет, поэтому механизм работает и при первой установке.
    * Squash-merge сохраняет tree PR только пока `develop` не ушёл вперёд.
      Если между PR CI и merge влился другой PR, merge-коммит имеет другой
      tree и `tree-<hash>` для него не существует. Тогда на push
      (`promote.mjs --allow-pr-head --unit <id>`) promotion находит PR
      коммита (`gh api repos/{repo}/commits/{sha}/pulls`, затем номер `(#N)`
      из squash-заголовка), берёт его head `H`, делает
      `git fetch origin refs/pull/N/head`, сверяет `H` с ответом API и
      считает `affected.compute` по ДЕЛЬТЕ `git diff H <merge-commit>`
      (имена файлов передаются через `--changed-file`; `--base H
      --head <merge>` нужен, чтобы scoping `MODULE.bazel`/`.bazelrc` читал
      правильные ревизии). Диапазон `H...merge` здесь неверен: он вернул бы
      и собственные изменения PR, уже содержащиеся в `H`. Если юнит в дельту
      не попал, его входы в merge-коммите идентичны проверенным на `H`:
      продвигается `tree-<tree(H)>` после обычной проверки provenance, в
      summary — `from PR head <H12>, delta to <merge12> does not affect
      <unit>`. Если дельта затрагивает юнит, источники и provenance не
      совпадают — job падает: подтянуть `develop` в PR, дождаться CI и
      смержить заново, либо `workflow_dispatch` после следующего PR. Для
      `home-deploy` и образов-соседей та же логика вместе с поиском по
      истории (соседи: сначала история, потом PR head). Native-юниты
      (Pulsar, logon provider) используют тот же fallback в
      `_native.yml`/`runNativePromotion`: release-shaped артефакт, собранный
      на head PR, продвигается, только если дельта не затрагивает
      native-юнит; иначе `native-release` падает. Любая ошибка поиска PR,
      fetch, сверки head или routing'а дельты — отказ (fail closed), а не
      разрешение.
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
      `<version>`. При отсутствии/непроверенном tree job падает с подсказкой
      перезапустить PR CI: `deploy-home` не собирает и не публикует
      native-артефакт. История для native не применяется (в отличие от
      образов): релизный артефакт — это и есть релиз, и старый tree нельзя
      перетегировать как текущий `VERSION`. PR-head fallback применяется:
      head PR — тот же релиз, если дельта до merge не затрагивает
      native-юнит; затронула — отказ. `deploy-station` только ставит
      версию через HA и проверяет digest'ы слоёв; сборки в нём нет.
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
      сейчас пуст (exporter Hermes переведён на дистролесс-образ `health-exporter`).
      Пакеты — образы деплой-юнитов и `native.publish_repository` из
      `units.mjs`; прочие пакеты аккаунта не рассматриваются. Для
      `pulsar-firmware` и `windows-logon-provider` это означает: релизные теги
      (`latest`, `vX.Y.Z`/`X.Y.Z`, `sha-<12>`) сохраняются, устаревшие
      `tree-*`-only удаляются через 7 дней.
    * Docker-образы тоже обязаны быть воспроизводимыми: одинаковые входы —
      один digest, иначе две сборки одной ревизии разойдутся, и деплой
      промоутнутого PR-образа будет отличаться от локальной проверки. Hermes
      собирается
      `projects/home-assistant/deploy/images/hermes/build.sh` с базовыми
      образами по digest, apt из `snapshot.debian.org` на дату базового тега,
      `SOURCE_DATE_EPOCH` = время upstream-коммита и Buildx
      `rewrite-timestamp=true`; `verify-reproducible.sh` доказывает совпадение
      двух сборок без кэша. Registry-кэш слоёв buildx
      (`ghcr.io/proflyder/hermes:buildcache`, запись `mode=max` только у job'ов
      с правом записи в GHCR) — ускорение: он переиспользует только слои с
      идентичными входами и digest не меняет. Тег и apt-снимок меняются только
      осознанным bump'ом.

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
    * Pod template несёт `proflyder.dev/rollout`, а
      `home-deploy render --rollout-key ROLLOUT_ID` заполняет его первыми
      12 hex sha256 **всего отрендеренного юнита** (digest'ы всех
      контейнеров, значения из private env, содержимое ConfigMap, probes,
      resources). При хешировании маскируются `BUILD_ID` (git-ревизия) и сам
      `ROLLOUT_ID`, поэтому хеш зависит только от того, что исполняют pods.
    * `BUILD_ID` остаётся аннотацией самого Deployment
      (`proflyder.dev/build`) для трассировки; изменение
      metadata не меняет generation и не катит pods. `converge
      --summary-file` пишет `unchanged|rolled-out` по сравнению
      `metadata.generation` до и после apply; `deploy-home` показывает это в
      job summary.
    * Изменение образа, отрендеренного конфига или смонтированного
      ConfigMap по-прежнему перезапускает pods. ESO-managed Secret'ы не
      входят в хеш: они не рендерятся юнитом.
    * Golden-фикстуры `render` (`--rollout-key`/`--fingerprint`) фиксируют:
      другая git-ревизия при тех же образах/конфиге → идентичный pod template;
      другой digest или конфиг → другой.


39. **Windows-провайдер — Rust на Bazel, без Visual Studio и отдельного mingw.**
    Провайдер, переносимый protocol-слой и `cp-selftest` — Rust;
    `windows-bindgen` генерирует закоммиченные `bindings.rs` по фильтрам из
    `bindings.txt`, поэтому в зависимостях только `windows-core` и
    `windows-link`, без крейта `windows`. `rules_rust` собирает
    `x86_64-pc-windows-gnu` и линкует герметичный zig `cc_toolchain`
    (mingw-w64/UCRT) из `MODULE.bazel`; host-код тестируется и линтуется
    `bazel test` на macOS/Linux, а Windows-цели — rustfmt/clippy через
    transition на `//platform/bazel/windows:x86_64_windows`. Два патча
    rules_rust нужны потому, что Bazel не отличает gnu-трипль от msvc по
    `@platforms`-constraints и rules_rust объявляет MSVC import library у
    любого Windows-cdylib; при обновлении rules_rust патчи перепроверить.
    PE-таймстемп обнуляется после сборки ради неизменяемых OCI-тегов, а
    импорты/экспорты проверяет `pe_artifacts.py` без Windows-тулчейна. CI
    ставит pinned Bazelisk вместо архива кросс-компилятора. MIT-фрагмент
    упаковки KERB — в `crates/provider/src/kerb.rs`.
    Сети провайдер достигает через WinHTTP (сетевой стек ОС внутри LogonUI),
    а не `reqwest`/`tokio`: это техническое исключение из общего стека
    крейтов (решение 40), а не отказ от него. Провайдер — член единого
    корневого Cargo workspace (решение 40), своих `Cargo.lock` и
    `rust-toolchain.toml` у решения больше нет. Переносимый слой использует
    стандартный стек: `serde`/`serde_json` (`deny_unknown_fields`),
    `base64`, `thiserror`, `http::Uri` для URL и `home-secret` для
    zeroizing-секрета (unix-часть `home-secret` под `cfg(unix)`, поэтому
    крейт собирается под `x86_64-pc-windows-gnu`). Журнал идёт через
    `tracing`, но с собственным слоем и Win32-писателем: формат строки,
    ротация 256 KiB и запись без stderr — контракт LogonUI, а не
    универсальный формат `home-telemetry`.

40. **Единый Rust workspace и общий core-слой.** Весь first-party Rust
    (Pulsar, BLE gateway, Windows logon provider, станционные helper'ы)
    живёт в одном корневом Cargo workspace: `/Cargo.toml`,
    `/Cargo.lock`, `/rust-toolchain.toml`, один `@crates//` в Bazel. Новый
    Rust-крейт добавляется членом этого workspace, а не отдельным
    workspace'ом; точные пины — только в `[workspace.dependencies]`,
    линты — в `[workspace.lints]` (Cargo и Bazel через
    `extract_cargo_lints` линтуют один набор).
    Код с двумя и более потребителями выносится в `packages/rust/`; пакет
    не импортирует `projects/**` и не содержит проектных endpoint'ов или
    путей БД. Общие примитивы станции и серверов — `home-runtime`
    (`Supervisor`: `spawn` для `Send`, `spawn_local` для `!Send`),
    `home-actor` (`Handle`/`channel`) и `home-linux` (errno, sysfs
    поверх `rustix`); они не привязаны к `!Send` и не диктуют рантайм.
    Стандартный стек крейтов вместо самописных реализаций:
    `tokio` (runtime/io/time), `tracing`, `thiserror`/`anyhow`, `clap`,
    `serde`/`serde_json`, `axum`, `reqwest` с `rustls`, `prost`,
    `zbus`, `base64`/`hex`/`subtle`, `tokio-util`. Самописные HTTP-,
    JSON-, CLI- и логгеры, ручной base64/hex и собственные примитивы
    синхронизации не добавляются.
    Технические исключения: Windows logon provider использует WinHTTP как
    сетевой стек ОС внутри LogonUI и не тянет `tokio`; DSP/FFI-код
    (ALSA, VQE, Sendspin, microWakeWord) остаётся на `libc`/`libloading` и
    `unsafe` по правилам Pulsar. Pulsar остаётся частью workspace;
    upstream-патчи адаптируются вручную: манифесты upstream мапятся на
    корневой workspace, версии переносятся в `[workspace.dependencies]`.
    Имя голосового ассистента не используется в идентификаторах кода
    (крейты, пакеты, модули): ассистента могут переименовать; общие крейты —
    `home-*`.

41. **Сервисы домашней платформы — Rust поверх общего core-слоя.**
    Долгоживущие first-party сервисы (scheduler, исполнители HA, voice-proxy,
    mcp-gateway, wakegrant, currency-bot) перепроектируются на Rust. Это не
    построчный перенос. Ресурсы — не мотив (все Go-поды вместе ≈72 Mi).
    Мотив — архитектура: одна платформа вместо копий конфига, healthcheck,
    backoff, SQLite и клиентов в каждом сервисе; типизированные контракты
    вместо `map[string]any`; чистый домен без I/O.
    * Слои: `packages/rust` — платформа и контракты; в solution — крейты
      `domain` (чистая логика, `Clock`, функции возвращают эффекты),
      `store`, `api`, `bin`. Wire-схема (prost) не служит доменной моделью
      и схемой хранения: преобразования живут в `api`/`store`.
    * Общие крейты по мере появления двух потребителей:
      `home-config` (env + `*_FILE` + границы значений),
      `home-sqlite` (single-writer, миграции), `home-queue` (durable
      outbox: dedupe → детерминированный id, lease/ack/attempts,
      идемпотентность), `home-worker` (цикл lease → handle → ack с
      backoff), `home-proto`, `home-clients` (HA, Telegram,
      Prometheus, Loki, Alertmanager; ошибка Transient/Permanent), а также
      `voice-contract` и `wakegrant-protocol` (общий с logon-provider).
    * Стек решения 40 плюс `rusqlite`, `rmcp`, `kube`, `russh`, `jiff`.
      RPC — официальный `connectrpc` (connect-rust) с рантаймом сообщений
      `buffa`. Один listener отдаёт Connect JSON/binary и gRPC-Web.
      `tonic` Connect-протокол не поддерживает. Для сервисов `home.v1`
      вместо `prost` используется `buffa`. Bearer
      проверяется через `connectrpc::Interceptor`, а не tower-слоем: иначе
      gRPC получает `Unknown` вместо `Unauthenticated`. Лимиты тела (1 MiB) и
      таймауты задаются явно.
      Серверы работают на multi-thread runtime, Pulsar явно строит
      `current_thread`.
    * Внешний контракт — `home.v1` (пути `/home.v1.*`), protojson,
      UUIDv5 и dedupe-ключи, схема БД v1→v2 (миграция на месте), имена env,
      имена и схемы MCP-инструментов, кадры SSE, метрики. Прежний
      контракт v1 не сохранялся как алиас: Android, HA, ingress verify и
      preflight переведены одновременно с сервером.
      Эталон — golden-фикстуры `testdata/contract/`; после удаления Go они
      остаются замороженными данными и проходят на Rust-реализации.
    * Итог: все сервисы — Rust на общем core-слое; Go в репозитории нет;
      языки — Rust, Kotlin, Python, JS/TS.

42. **Менеджер паролей — self-hosted Vaultwarden.** Для доступа человека к
    паролям и копии секретов на устройствах используется Vaultwarden в K3s
    (`passwords.<home-domain>`, только LAN/VPN), а не облачный Bitwarden или
    1Password: бесплатно, без лимитов на вложения, данные дома. Офлайн-кэш
    клиентов Bitwarden и ежедневный offsite-бэкап закрывают потерю сервера.
    OpenBao остаётся источником правды для машинных секретов; Vaultwarden
    получает их копию отдельной синхронизацией. Регистрация закрыта, egress
    только DNS, `/admin` включён с Argon2id-хэшем токена из OpenBao.

43. **MCP-контракт mcp-gateway сверяется семантически.** `initialize`,
    `tools/list` и `tools/call` сравниваются по смыслу: объекты как
    подмножества (имена инструментов, описания, `inputSchema`-свойства,
    аннотации `readOnlyHint`/`destructiveHint`, `speech`/`details` важны),
    массивы — точно, SSE-обрамление `event: message\ndata: …\n\n` —
    побайтно. Поля Go SDK `ttlMs`, `cacheScope`, `structuredContent` и
    `idempotentHint:false` не входят в ожидаемое подмножество, потому что
    `rmcp` их не эмитит. Байтовыми остаются auth, `/healthz`, `/readyz`,
    серии `mcp_gateway_*` в `/metrics` и `/v1/voice/*`. Режим реализован
    в `packages/rust/home-contracttest` (`compare: semantic`,
    `response.format: sse`). Справочный байтовый снимок Go-эпохи —
    `solutions/mcp-gateway/testdata/reference/mcp_tools_list_go_snapshot.json`.

44. **Рабочее место и CI — из `mise.toml`, секреты — из OpenBao.** Версии
    инструментов (Python, Node, Bazelisk, gitleaks, actionlint, kubectl,
    helm, oras, bao, gh, bw, protoc и др.) задаёт один `mise.toml`, контрольные
    суммы — `mise.lock` (linux-x64, macos-arm64). CI ставит нужное подмножество
    через `jdx/mise-action`; Bazel остаётся герметичным (`MODULE.bazel`), его
    ставит `setup-bazel`. Новая машина: `mise run setup` — человек входит в
    OpenBao (пароль + TOTP), машина получает свой periodic operator token,
    SSH-ключи и приватные заметки приходят из `secret/infra/*`. Каталог путей
    (только имена) живёт в Git (`platform/workstation/catalog.json`,
    `docs/SECRETS.md`) и сверяется с OpenBao (`mise run secrets:check`).

45. **Публичный TLS — системный CA-бандл, не фичи `reqwest`.** Корневой
    `reqwest` собирается с `rustls-no-provider`: прошивка станции
    (`pulsar-update`) сама задаёт корни через `tls_certs_only`, а Cargo
    унифицирует фичи всего workspace, поэтому включать встроенные
    `rustls`-корни нельзя. Общий крейт `packages/rust/home-tls` читает PEM-бандл
    (`SSL_CERT_FILE`, иначе `/etc/ssl/certs/ca-certificates.crt`, fallback
    macOS `/etc/ssl/cert.pem`), задаёт его через `tls_certs_only` и ставит
    ring provider; пустой/отсутствующий
    бандл — ошибка на старте. Клиенты, ходящие в публичный HTTPS
    (currency-bot → kurs.kz/Telegram, `home-telegram`, `home-clients`),
    строятся только через `home_tls::public_https_client()`.

46. **Определения сценариев хранит scheduler, исполняет `ha-scenario`.**
    scheduler — единственный владелец живых расписаний (решение 10), поэтому
    `ScenarioService` живёт в нём: определения и revision в SQLite (схема v2),
    time-триггеры материализуются строками `schedules`
    (`kind=SCHEDULE_KIND_SCENARIO`, очередь `scenario`, `task_type` —
    `scenario.fire`), `RunScenario` кладёт manual-событие в ту же очередь.
    Условия и шаги scheduler только хранит и валидирует; вычисляет и исполняет
    отдельный бинарь `ha-scenario` в `ha-bridge`. Второго планировщика и
    второго владельца определений нет. `GetStatus` различает
    `scenario_store_supported=true` (хранилище есть) и
    `scenario_execution_supported` — deployment-флаг scheduler
    (`SCENARIO_EXECUTION`), который включается вместе с исполнителем, а не
    угадывается по живым подам.

47. **Бытовые сценарии — нативные определения scheduler, а не HA-скрипты.**
    «Блэкаут» и «Сел работать» хранятся в `home.v1.ScenarioService` и
    исполняются `ha-scenario`; голос вызывает `RunScenario` через
    `script.scenario_run` по точной метке (без учёта регистра), а не выбирает
    светильники в HA. Сид `scheduler/scenarios/seed.mjs` идемпотентен по
    метке: создаёт отсутствующие (`idempotencyKey=seed-<slug>`) и никогда не
    перезаписывает существующие определения, чтобы правки владельца в
    приложении сохранялись. Новые бытовые сценарии добавляются так же, а не
    новым `script.*` с меткой `scenario`; метка `scenario` остаётся для
    HA-сценариев, которым нужны сервисы вне каталога (обучение Рикардо).

48. **Секреты для локальных скриптов — fnox, единая точка входа `mise run admin:*`.**
    Скрипты деплоя и обслуживания с рабочего места (ingress, OpenBao,
    Vaultwarden, observability, ESO, heartbeat, offsite, backup scheduler,
    ручной deploy сервисов, Android-релиз, `deploy:status --live`,
    `rollback --yes`) запускаются только через задачи mise. `fnox.toml` в
    корне содержит лишь ссылки на OpenBao: профиль на скрипт, файловые
    секреты — во временных файлах 0600, удаляемых после команды, в тех
    переменных, что скрипт уже читает. fnox ходит в OpenBao через shim
    `vault` → `bao` и токен машины (`workstation.py token`, с продлением);
    туннель к API K3s — `with-kube-tunnel.sh` на время команды.
    `secrets:file` остаётся запасным путём. `rclone.conf` офсайта намеренно
    не передаётся (на узле rclone обновляет OAuth-токен сам).

Меняется решение — обновить этот файл и `STATE.md`.

48. **Звонок телефон ↔ колонка — WebRTC, сигналинг `solutions/intercom`.**
    Медиа идёт P2P по UDP (DTLS/SRTP, Opus); кластер не пересылает звук.
    `home.v1.IntercomService` держит только эфемерное состояние звонка и
    релеит SDP/ICE. Транспорт — Connect по HTTP/1.1: unary вызовы и
    server-streaming `Watch` (Android уже говорит Connect по HTTP/1.1;
    bidi требует HTTP/2, поэтому `SendSignal` — отдельный unary). Контракт
    симметричный: `StartCall`/`Answer`/`Hangup` не предполагают направление
    «телефон → колонка», входящий звонок на телефон — будущий клиент.
    Identity endpoint'а берётся из токена (phone bearer / station device
    token), а не из запроса; звонок один на endpoint, RINGING 30 с,
    CONNECTING 20 с, ENDED-сессия живёт 60 с. TURN не обязателен: телефон и
    колонка в LAN/VPN; `GetIceConfig` уже отдаёт список STUN/TURN из
    конфигурации, поэтому coturn добавляется без смены клиентов. Pulsar
    получает `pulsar-call` на `str0m` и `Phase::Call` отдельным этапом;
    scheduler и `ha-bridge` в звонке не участвуют.

49. **Кэши Actions пишет только `develop`, по семействам (Android и host-Rust).**
    PR-прогоны кэши только восстанавливают (`actions/cache/restore`): GitHub
    привязывает запись к ref'у, который её создал, поэтому PR-кэш не виден
    другим PR'ам, но занимает общий лимит 10 ГБ и минуты на загрузку. PR-кэши
    диска больше не создаются. Единственные писатели develop-стороны —
    `warm-android` и `warm-rust` в `deploy-home` (push в `develop`). Кэши
    разделены по семействам, потому что наборы целей и скачиваний разные, а
    лимит общий: у Android только disk cache
    `bazel-android-<os>-warm-<hash>` (его скачивания — в основном Maven JAR из
    Nexus, отдельный repository archive не окупается), у host-Rust
    `bazel-repo-rust-<os>-<hash>` и `bazel-rust-<os>-warm-<hash>`. `warm-android`
    собирает все build/test-цели android-юнитов с `--config=android`;
    `warm-rust` — все build/test-цели `bazel`-юнитов без `--config=android`
    (ровно флаги `_unit.yml`, иначе configuration checksum version defines не
    совпал бы). Repository cache хранит только скачанные архивы
    (`content_addressable/`), не их распаковку (`contents/`), и `warm-rust`
    восстанавливает его только по точному ключу: смена hash начинает с пустого
    каталога, старые версии не накапливаются. `bazel fetch` не используется —
    он тянет все зарегистрированные toolchain независимо от семейства (так
    общий кэш и вырос до 8 ГБ); `bazel build` фетчит только нужное. Дисковый
    кэш ограничен `--experimental_disk_cache_gc_max_size` (5G Android, 3G
    Rust). У юнитов без develop-писателя (`_native.yml`,
    `_deploy-control.yml`) кэш только восстанавливается. `_native.yml` берёт
    repository-архив семейства rust (общие крейты/toolchain), свои дисковые
    ключи (`bazel-logon-`, `bazel-pulsar-`, `bazel-installer-`) остаются
    отдельными из-за кросс-платформенных toolchain.
    Отдельного сужения целей нет: каждая запись матрицы гоняет объявленные
    цели, незатронутые тесты Bazel берёт из кэша (job `plan` с
    target-determinator удалён — он тратил ~5 мин на двойной анализ графа и
    задерживал старт Android-джоб). Распространяемый APK собирается ровно
    один раз — в matrix-записи `android-app` (`_android.yml`) без секретов:
    `google-services.json` не секрет (идентификаторы Firebase открыто лежат в
    каждом APK) и берётся из переменной репозитория, тесты идут на том же
    `--define`-варианте APK, результат — неподписанный artifact
    `android-app-apk`. Релизный ключ есть только у `android-distribute`
    (Environment `Android`, роль OpenBao `android-distribute`): он подписывает
    готовый APK и загружает его в Firebase. Так основная проверка не зависит
    от Environment/VPN, а PR из форков проходят её как обычно.

50. **Android-only Bazel-флаги — в `platform/android/android.bazelrc` под
    `--config=android`.** Корневой `.bazelrc` — глобальный вход планировщика
    (`platform/ci/units.mjs`): любая правка запускает все юниты. Поэтому
    Android-специфичные опции (version defines `home_version_code`/
    `home_version_name`, Java-language/toolchain build-инструментов,
    `--host_javacopt`, protoc toolchain, `--persistent_android_dex_desugar`,
    в будущем `--android_platforms`) вынесены в
    `platform/android/android.bazelrc`; корневой файл подключает его через
    `try-import`, а активирует `--config=android`. Файл лежит под
    `platform/android/**`, поэтому его правка выбирает только android-юниты.
    `--config=android` передают все точки сборки Android: диспетчер юнитов
    (`mise run build|test` для android-юнитов, а также `mise run build` и
    `mise run test:bazel` на весь workspace), `distribute.sh` и
    Android-сборка в `deploy-home` → `warm-android`. В `distribute.sh`
    `--config=android` идёт до `--define`, чтобы versionCode сборки победил
    дефолт из конфига. Без конфига сборка падает на анализе
    (`$(home_version_name) not defined`), а не собирает молча APK другого
    ABI; ABI дополнительно проверяет `apk_abi_test`.

51. **Проверки и образы юнита — один job.** `_unit.yml` заменил пару
    `_bazel.yml` (диск-кэш, `mise run build/test`) и `_images.yml`
    (холодный Bazel-сервер, повторная сборка `<image>_image`, push). Теперь
    один job восстанавливает кэш, гоняет проверки и собирает образы в том же
    Bazel-сервере, затем `platform/ci/build-images.sh` пушит `tree-<hash>` и
    тот же job аттестует каждый digest статическими шагами (по одному на
    индекс образа, максимум 4; `actions/attest-build-provenance` принимает
    ровно один OCI-subject за вызов, а `subject-checksums` не умеет
    `push-to-registry`). Отдельного `attest`-job'а и `_attest.yml` нет.
    Это убирает вторую сборку Rust-сервиса и второй job на типичном
    сервисном PR. `kind: python` покрывает Hermes: его Python-suite и mixed
    Buildx-образы идут в том же job, а не в отдельных `python` и `images`.
    Python-юниты без образов схлопнуты в один `python` job: один
    checkout/mise/pip и последовательный цикл `mise run test <id>` с
    `::group::`, вместо ~22 коротких job'ов. `ci-ok` ждёт `unit`+`python`
    вместо `bazel`+`images`+матрицы `python`.

52. **CI читает артефакты через домашний Nexus.** `projects/home/kubernetes/nexus`
    — один Nexus Repository OSS `3.76.1` (последний до лимитов Community
    Edition) в namespace `nexus`, digest-пин, один PVC `50Gi local-retain`,
    JVM `-Xms/-Xmx 1g` и `MaxDirectMemorySize 1g`. Провижининг — идемпотентный
    Job по REST API (`provision.sh`): blob store, `maven-central` +
    `google-maven` + группа `maven-public`, raw-прокси `github` и
    `nodejs-dist`, npm и pypi, cargo — опционально (если формат недоступен,
    Job продолжает работу). Анонимный доступ выключен, у CI только роль
    `ci-reader` (`read`/`browse`); пароли `admin` и `ci-reader` — в OpenBao
    `secret/k8s/nexus/nexus-secrets` через ESO, в Git и логи не попадают.
    Публично открыт только `/repository/*`: отдельный IngressRoute без
    `home-allowlist`, с лимитом тела 8 MiB и rate limit; UI и
    `/service/rest/*` остаются на внутреннем имени и allow-list. Публичное
    имя обслуживает существующий Cloudflare Tunnel, отдельный входящий порт
    не открывается. CI включает кэш одним переключателем — repository
    variable `NEXUS_HOST` + secret `NEXUS_CI_PASSWORD`; composite action
    пишет `~/.netrc`/`~/.npmrc`/pip.conf и рендерит
    `platform/bazel/downloader.cfg` в `~/.bazelrc`, но только если
    `/service/rest/v1/status` отвечает 200. Без переменной, без секрета
    (форк-PR) или при недоступном Nexus сборка идёт напрямую, как раньше;
    локальные сборки этот файл не читают. `rules_jvm_external` пробует
    Nexus-группу первой, затем публичные URL.

53. **Чувствительная инфраструктура — pull-based GitOps на Flux.** Пилот —
    Nexus (`projects/home/kubernetes/flux/`, `projects/home/kubernetes/nexus/`).
    В кластере только `source-controller`, `kustomize-controller` и
    `helm-controller` Flux `v2.10.0` (без notification/image automation),
    манифесты зафиксированы в репозитории и ужаты по CPU/памяти под один
    домашний узел. `GitRepository` смотрит на `develop` через read-only
    deploy key из OpenBao (`secret/k8s/flux-system/git-deploy-key`); в CI нет
    admin-доступа к кластеру. Приватные `NEXUS_HOST`, `NEXUS_PUBLIC_HOST` и
    `NODE_LAN_IP` приходят из `secret/k8s/flux-system/settings` и
    подставляются в `postBuild`. Провизор Nexus остался Job'ом, но его
    ConfigMap хешируется по содержимому `provision.sh`, поэтому изменение
    скрипта перезапускает провижининг; Cloudflare DNS — ручной шаг.
    Остальные namespace, OpenBao и сервисы приложений остаются на
    `deploy-home`.
