# DECISIONS — ключевые решения

Формат: решение — причина — ограничение. Номера стабильны: на них ссылаются
код и CI (`DECISIONS 36`, `54`, `55`, `60` и др.), не перенумеровывать.
Факты — `STATE.md`; восстановление — `REBUILD.md`; секреты — `SECRETS.md`.
Меняется решение — обновить этот файл и `STATE.md`.

1. **Pulsar вместо штатной Алисы.** Полный контроль, локальный STT/TTS,
   HA Assist, egress-quarantine. `maind` остановлен; гибрид не поддерживается.
2. **Proxmox + HAOS VM + Ubuntu/K3s VM.** HAOS — Supervisor/add-ons;
   K3s — voice/Hermes/observability; обновляются независимо.
3. **Wi-Fi 4-address/WDS uplink, Ethernet fallback.** `vmbr0` сохраняет L2,
   VM IP/URL не меняются, NAT/PCI passthrough не нужны. Оба uplink
   одновременно в мост не подключать.
4. **Собственный VPN + приватные HTTPS-имена, не публичный HA.** Наружу —
   только `443/tcp+udp` на ocserv (сертификат или пароль + TOTP). Сервисы —
   Traefik в K3s, wildcard Let's Encrypt (DNS-01) под приватным суффиксом,
   allow-list LAN/VPN; DNS указывает на LAN-адрес.
5. **STT/TTS локально.** GigaAM primary, Whisper fallback, Piper; Hermes —
   только свободная речь. Облако видит текст, но не аудио.
6. **Hermes не владеет state.** Расписания — scheduler (решение 10),
   громкость/музыка — HA; локальные intents работают без LLM.
7. **MA + Spotify Connect.** MA — локальный хаб, Connect — телефон, Pulsar —
   Sendspin-плеер. Один аккаунт = один stream-slot.
8. **BLE gateway — аварийный provisioning.** Работает без Wi-Fi и облака
   (GATT: SSID/PSK/token).
9. **OTA/ADB fortress + egress guard.** Защита от OTA, сброса и закрытия ADB;
   публичный WAN и Яндекс закрыты. Factory reset сносит защиту.
10. **Расписания — canonical scheduler, звук — HA.** `solutions/scheduler`
    владеет будильниками/таймерами/напоминаниями; `ha-bridge` исполняет
    очередь `home`; голос и Android работают через Connect API.
11. **Recovery — локально + offsite** (HA/K3s/PVE/станция, проверка SHA-256).
    Factory-образа Midi нет; OTA-блок не заменяет recovery.
12. **Монорепозиторий + Bazel.** Hermetic build, тесты рядом с кодом. Языки —
    решения 35 и 41.
13. **Home-сценарии — метка HA `scenario`.** Скрипт попадает в каталог только
    с этой меткой; иконки/комнаты/имена — лишь отображение.
14. **Reminder-грамматика — один слот `{text}`** до или после даты/времени;
    два фрагмента вокруг даты осознанно не распознаются.
15. **SSH станции — ключ, пароль как аварийный fallback.** ADB/SSH — только LAN.
16. **Источник истины расписаний — scheduler, не HA-helper'ы.** Заметки
    остаются в `todo.notes`.
17. **Сервисы K3s — digest-пины образов.** Bazel `rules_oci` (distroless,
    base по digest), `ghcr.io/proflyder/<name>@sha256:…`,
    `imagePullPolicy: IfNotPresent`. Применяет Flux (решения 53–55).
18. **OpenBao — единственный секрет-сервер.** Raft на локальном Retain-PV,
    audit в stdout → Loki (полный диск не блокирует запросы). Auto-unseal —
    встроенный `static` seal с ключом в Secret на том же узле: перезагрузка
    без человека; риск принят владельцем, внешний KMS/HSM/TPM не вводим; ключ
    не входит в snapshot и хранится offsite отдельно. Человек — `userpass` +
    TOTP, `admin` без `sys/seal`/`raw`/`rekey`; CI — GitHub OIDC только для
    станции (`deploy-station`, TTL ≤ 15 мин) и Android; поды — Kubernetes auth
    (`secret/k8s/*`). Root-токен отозван, recovery keys — офлайн.
19. **Git — только PR, squash и `ci-ok`.** Merge запускает `deploy-home`
    (promotion + pins), применяет Flux. Откат — revert pins/PR, не
    `kubectl rollout undo`.
20. **Pulsar — релизы из монорепо, выкатка через HA updater.** Любое
    смерженное изменение `projects/station/pulsar/**` — релиз: `native-release`
    продвигает проверенный PR-артефакт `tree-<hash>` на
    `ghcr.io/proflyder/pulsar-firmware:<version>` и `:latest` (решения 36, 60);
    GitHub Releases и теги не используются. `deploy-station` после approve в
    Environment `Station` идёт через VPN и OIDC, вызывает HA `update.install` и
    проверяет версию. Откат — на станции (`.prev` + `ctl.sh reload`), не CI.
    Egress станции: HTTPS только к диапазонам GitHub, остальной WAN — DROP.
21. **Currency bot — Rust, Cloudflare Tunnel, токены в OpenBao.** SQLite,
    webhook на отдельном порту с Telegram `secret_token` и allowlist чатов.
    Наружу — только путь webhook через Tunnel; API — LAN/VPN. Деплой — Flux
    по pins (решение 54).
22. **Один MCP-шлюз, перезапуск только с подтверждением.** Инфра-диагностика —
    только через `solutions/mcp-gateway`; у Hermes нет SA-токена и kubectl.
    Группы `infra.read`, `infra.fix` (единственная мутация — rollout restart
    из каталога, prepare → confirm, TTL 90 с, cooldown 5 мин) и `infra.report`
    (фиксированный получатель). Self-disrupting цели (`hermes`, `mcp-gateway`)
    патчатся с задержкой, чтобы ответ успели озвучить. HAOS/MA/станция —
    только чтение. RBAC `patch` с `resourceNames` обязан совпадать с каталогом
    (contract-тест). Отчёты — отдельный инфра-бот.
23. **Внешний dead-man switch — Cloudflare Worker (`solutions/heartbeat`).**
    Alertmanager шлёт `Watchdog` в Worker, тишина дольше grace уходит в
    Telegram тем же инфра-ботом — без стороннего мониторинга. Компромисс:
    токен бота хранится ещё и в Worker secrets; при ротации обновлять оба места.
24. **Offsite backup — Google Drive + restic, ключи отдельно.** Drive без
    привязки карты (перерасход невозможен), scope `drive.file`; R2 отклонён —
    нет жёсткого лимита расходов. Шифрование на узле, лимиты 2 GiB за запуск и
    5 GiB на репозиторий. Вывозится только восстановимое (raft snapshot,
    SQLite, ledger, зашифрованный backup HA); `secrets/`, CA, VPN-ключи,
    seal/recovery keys — никогда (скрипт отказывает `*seal*`/`*recovery*`/
    `*.key`). Пароль restic — в password manager вне дома.
25. **Голосовой профиль — ручной режим.** Истина —
    `input_select.home_voice_profile`, меняется только явно (голос, дашборд,
    Android). Не выводится из фразы/времени, автовозврата нет; влияет только на
    свободную речь. Ошибка чтения — отказ, не догадка.
26. **Hermes видит только личный профиль bb.** Рабочие и неизвестные проекты
    отфильтрованы шлюзом и неотличимы от несуществующих. Запуск агента —
    prepare → подтверждение → confirm (TTL 90 с).
27. **Рабочая речь — только в рабочий Claude, fail-closed.** Ошибки bb не
    откатываются в Hermes.
28. **bb доступен домашним сервисам только через плагин и SSH forced
    command.** HTTP плагина слушает loopback на CT 103 (токен + HMAC);
    `mcp-gateway` ходит по SSH с ограниченным ключом, pinned `known_hosts` и
    forced command. Маршрут через bb Connect закрыт.
29. **Встроенный MCP bb не используется для голоса:** он all-or-nothing —
    нельзя ограничить проекты/профиль и выдать короткий отказ. Вместо него —
    тонкий плагин с явным контрактом.
30. **Свой credential provider вместо Windows autologon.** Прошивка не
    отличает WoL от кнопки; вход разрешает HA одноразовым грантом до WoL.
    Без гранта — обычный вход по PIN. `solutions/windows-logon-provider`.
31. **Грант только по назначению: `logon` — после холодной загрузки,
    `unlock` — заблокированная сессия того же пользователя.** Режим — чистая
    функция от сценария, WTS-сессии и аптайма (Windows 11 вызывает
    `CPUS_LOGON` и при блокировке). MAC включает purpose, одно назначение не
    выполняет другое; ошибка запроса — отказ.
32. **`wakegrant` — отдельный сервис, не часть `mcp-gateway`.** Plain HTTP в
    LAN с HMAC в обе стороны поверх серверного nonce, fail-closed; грант
    одноразовый, 180 с, SQLite. Контракт —
    `packages/contracts/wakegrant/PROTOCOL.md`.
33. **Заменено решением 39.**
34. **Windows-бинарники — публичные OCI-артефакты в GHCR через ORAS.**
    Установка по digest после `sha256sum -c` и `gh attestation verify`. Тег
    версии неизменяем по содержимому. GitHub Releases отклонены (смешали бы
    продукты монорепо), Maven/NuGet — требуют авторизации даже на чтение.
    Новый пакет GHCR может создаться приватным — публичность включается один
    раз в UI.
35. **Язык — по месту исполнения.** Сервисы K3s и деплой-инструменты — Rust
    (один конвейер Bazel → OCI). Хостовая обвязка до ~200 строк без протоколов
    и криптографии — shell или Python stdlib, больше — Rust-бинарник по digest.
    CI-обвязка — JS (`actions/github-script`, модули `platform/ci/`,
    `node --test`, без npm). Инструменты сборки Bazel — язык стека в
    exec-конфигурации (Android — Kotlin), сборка не зависит от `python3` хоста.
    Python — ML wake-word, dev-утилиты станции, проверки YAML, live-e2e.
    Существующие операторские Python-скрипты переписываются только вместе с
    существенной переделкой. Go в репозитории нет.
36. **Артефакт собирается один раз на PR; деплой — по git tree hash и
    provenance.**
    * Идентичность — `tree-<tree hash>` собираемого merge-ref: совпадает с tree
      squash-коммита, пока `develop` не ушёл вперёд. Одно правило для образов и
      native-артефактов.
    * PR из этого репозитория публикует образы (`_unit.yml`) и release-shaped
      native-артефакты (`_native.yml`) под `tree-<hash>` и аттестует digest
      (`actions/attest-build-provenance`); форки только собирают.
    * `deploy-home` ничего не собирает: берёт существующий `tree-<hash>` с
      проверенным provenance (signer — workflow этого репозитория), иначе job
      падает с причиной в summary (`DEPLOY_NO_BUILD=1`).
    * Если `develop` ушёл вперёд, promotion берёт head PR, если дельта
      `git diff <head> <merge>` не затрагивает юнит; иначе отказ. Для образов
      вне текущего tree (dispatch, соседи) — поиск по first-parent истории.
      Любая ошибка поиска — fail closed.
    * Native-теги (`<version>`, `sha-<12>`) неизменяемы по содержимому: тот же
      набор слоёв — no-op, другой — отказ. `latest` указывает на digest тега
      версии. История для native не применяется.
    * Любой job, пишущий в GHCR, логинится до первой записи (публичные пакеты
      читаются анонимно, падает только PUT). Проверяет `check_units.mjs`.
    * `cleanup.yml`: GHCR — хранить новейшую, `develop`/`latest`/semver/
      `sha-*`, `PROTECTED_DIGESTS`, моложе 7 дней и пять последних
      deploy-версий; Firebase App Distribution — окно 24 ч, новейший релиз не
      удаляется. Уборка не входит в `ci-ok`.
    * Docker-образы обязаны быть воспроизводимыми (base по digest, apt-снимок,
      `SOURCE_DATE_EPOCH`, `rewrite-timestamp`), иначе промоутнутый образ
      разошёлся бы с локальной проверкой.
37. **Pulsar — first-party код в монорепо** (AGPL-3.0). Upstream вливается
    вручную патчем (`AGENTS.md`), автоматического fork-sync нет. Имя пакета
    `proflyder/pulsar-firmware` зашито в `pulsar_update::UPDATE_REPO`
    установленной прошивки — не переименовывать. Релиз — решение 20.
38. **Pod template аннотируется хешем отрендеренного юнита, а не
    git-ревизией** (`proflyder.dev/rollout`, `BUILD_ID` маскируется). Причина:
    ревизия перезапускала все pods на каждом merge. ESO-Secret'ы в хеш не
    входят.
39. **Windows-провайдер — Rust на Bazel без Visual Studio.**
    `x86_64-pc-windows-gnu` + герметичный zig `cc_toolchain`; `bindings.rs`
    генерируются `windows-bindgen` и закоммичены. Два патча rules_rust (Bazel
    не отличает gnu от msvc) — перепроверять при обновлении. PE-таймстемп
    обнуляется ради неизменяемых тегов. Сеть — WinHTTP, не `reqwest`/`tokio`:
    техническое исключение из решения 40 (стек ОС внутри LogonUI). Журнал —
    свой слой `tracing` (контракт LogonUI, без stderr).
40. **Единый Rust workspace и общий core-слой.** Один `/Cargo.toml`,
    `/Cargo.lock`, `/rust-toolchain.toml`, `@crates//`; пины — в
    `[workspace.dependencies]`, линты — `[workspace.lints]`. Код с двумя
    потребителями — `packages/rust/` (без `projects/**` и проектных
    endpoint'ов). Стандартные крейты (`tokio`, `tracing`, `thiserror`/`anyhow`,
    `clap`, `serde`, `axum`, `reqwest`+`rustls` и т.д.) вместо самописных.
    Исключения: WinHTTP в logon provider, `libc`/`unsafe` в DSP/FFI Pulsar.
    Имя ассистента не используется в идентификаторах — общие крейты `home-*`.
41. **Сервисы платформы — Rust поверх общего core-слоя.** Мотив —
    архитектура, не ресурсы: общие config/healthcheck/backoff/SQLite/клиенты,
    типизированные контракты, чистый домен без I/O. Слои solution: `domain`,
    `store`, `api`, `bin`; wire-схема не служит доменной моделью и схемой БД.
    RPC — `connectrpc` + `buffa` (Connect JSON/binary и gRPC-Web на одном
    listener; `tonic` Connect не умеет). Bearer — через
    `connectrpc::Interceptor`, иначе gRPC получает `Unknown` вместо
    `Unauthenticated`. Эталон контракта — golden-фикстуры `testdata/contract/`.
42. **Менеджер паролей — self-hosted Vaultwarden** (LAN/VPN): бесплатно,
    данные дома. OpenBao остаётся источником машинных секретов, Vaultwarden
    получает копию. Регистрация закрыта, egress только DNS.
43. **MCP-контракт mcp-gateway сверяется семантически** (объекты —
    подмножества, массивы — точно, SSE-обрамление — побайтно): `rmcp` не
    эмитит часть полей прежнего SDK. Auth, health, `/metrics`, `/v1/voice/*` —
    побайтно. Реализация — `packages/rust/home-contracttest`.
44. **Рабочее место и CI — из `mise.toml`, секреты — из OpenBao.** Версии и
    `mise.lock` — одна точка; Bazel герметичен через `MODULE.bazel`. Новая
    машина — `mise run setup`. Каталог путей (только имена) — в Git,
    сверяется `mise run secrets:check`.
45. **Публичный TLS — системный CA-бандл через `home-tls`.** `reqwest` с
    `rustls-no-provider`: прошивка задаёт свои корни, а Cargo унифицирует фичи
    workspace, поэтому встроенные корни включать нельзя. Пустой бандл — ошибка
    на старте; публичные клиенты — только `home_tls::public_https_client()`.
46. **Определения сценариев хранит scheduler, исполняет `ha-scenario`.**
    Один владелец расписаний (решение 10): time-триггеры — строки `schedules`
    очереди `scenario`. Scheduler только валидирует. `GetStatus` различает
    наличие хранилища и deployment-флаг `SCENARIO_EXECUTION`, а не угадывает
    по живым подам.
47. **Бытовые сценарии — нативные определения scheduler, не HA-скрипты.**
    Голос вызывает `RunScenario` по точной метке. Сид идемпотентен и не
    перезаписывает существующие определения (правки владельца сохраняются).
48. **Секреты для локальных скриптов — fnox через `mise run admin:*`.**
    `fnox.toml` — только ссылки на OpenBao; файловые секреты — во временных
    файлах 0600, удаляемых после команды. `secrets:file` — запасной путь.
    `rclone.conf` офсайта не передаётся (rclone на узле обновляет OAuth сам).
48a. **Звонок телефон ↔ колонка — WebRTC P2P, сигналинг `solutions/intercom`.**
    Кластер не пересылает звук; сервис держит эфемерное состояние и релеит
    SDP/ICE. Connect по HTTP/1.1 (unary + server-streaming `Watch`), потому что
    bidi требует HTTP/2. Identity — из токена, не из запроса. Контракт
    симметричный; TURN добавляется конфигом `GetIceConfig` без смены клиентов.
49. **Кэши Actions пишет только `develop`** (`warm-android`, `warm-rust` в
    `deploy-home`); PR только восстанавливают: PR-кэш не виден другим PR, но
    занимает общий лимит 10 ГБ. Семейства разделены, `bazel fetch` не
    используется (тянет все toolchain), disk cache ограничен GC. APK
    собирается один раз без секретов (`google-services.json` не секрет);
    подпись и Firebase — только в `_android-release.yml` с Environment
    `Android`, поэтому PR из форков проходят основную проверку.
50. **Android-only Bazel-флаги — `platform/android/android.bazelrc` под
    `--config=android`.** Корневой `.bazelrc` — глобальный вход планировщика
    (правка запускает все юниты). Без конфига сборка падает на анализе, а не
    собирает молча APK другого ABI.
51. **Проверки и образы юнита — один job** (`_unit.yml`): нет второй сборки и
    холодного Bazel-сервера. Attestation — статические шаги по индексу образа
    (action принимает один OCI-subject). Python-юниты без образов — один job.
52. **CI читает артефакты через домашний Nexus.** Наружу (через Cloudflare
    Tunnel) — только `/repository/*` с лимитами; UI/REST — за allow-list. У CI
    только роль чтения. Без `NEXUS_HOST`/секрета (форки) или при недоступном
    Nexus сборка идёт напрямую.
53. **Чувствительная инфраструктура — pull-based GitOps на Flux.** Только
    source/kustomize/helm-controller, read-only deploy key из OpenBao; у CI
    нет admin-доступа к кластеру. Cloudflare DNS — ручной шаг.
54. **Все деплой-юниты приложений катит Flux, CI только пинует digest'ы.**
    Job `pins` проверяет provenance и коммитит digest в ветку
    `deploy/home-apps`; Flux читает её как `ConfigMap/home-app-images`. У CI
    нет VPN/OpenBao/kubeconfig на этом пути. Режимы исполнителей — в
    Git-патче, не side effect деплоя. Откат — `flux suspend` + revert pins;
    PVC несут `kustomize.toolkit.fluxcd.io/prune: disabled`.
55. **Входящего доступа CI в K3s нет.** `deploy-home` не применяет объекты
    (jobs: `plan`, `pins`, `warm-*`, `native-release`, `deploy-station`).
    VPN-группа `ci` и `443` остаются только для `deploy-station`. Аварийные
    admin-инструменты (`scheduler/deploy.sh`, `hermes rollout`,
    `external-secrets/deploy.sh`) сохранены; штатный путь — merge → pins → Flux.
56. **Wake word обучается только с нуля** (`--from-scratch`): дообучение во
    всех экспериментах ухудшало recall, малый LR и замороженный BatchNorm не
    помогают. Рецепт — `WAKE-WORD.md`.
57. **Конфиг Hermes — content-addressed образ `hermes-config`.** Digest и есть
    checksum входов; initContainer рендерит fail-closed, sidecar меряет drift.
    Pod перезапускается только при изменении overlay/рендерера. Плановые
    остановки — тихий drain в preStop, без «shutting down» в домашнем канале.
58. **Конфиг HA доставляется in-cluster pull-синхронизацией
    (`solutions/ha-config-sync`), а не CI-over-VPN.** У CI нет постоянного пути
    в LAN, а разовый job не лечит drift. Применение — staging, бэкап,
    `check_config`, откат при ошибке; restart HA — только вне звонящего
    будильника. `configuration.yaml`, `secrets.yaml`, `scenes.yaml` недоступны.
59. **Один триггер на воркфлоу, логика — в reusable.** `github.event_name`
    внутри reusable — событие вызывающего, ветвление по нему давало неверные
    ветки; вызывающий передаёт явные inputs. `ci-ok` остаётся job в `ci.yml`,
    имя `deploy-home.yml` не меняется (внешние контракты). Проверяет
    `check_units.mjs`.
60. **Версия native-юнитов вычисляется, а не бампается.** `VERSION` —
    `major.minor`; patch — число коммитов по путям юнита в `HEAD^1`
    (`platform/ci/version.mjs`). Merge-ref и squash-commit имеют одного первого
    родителя, поэтому версия PR-бинарника совпадает с релизной; promotion берёт
    её из аннотации `org.opencontainers.image.version`. Релиз — любое
    смерженное изменение юнита. Релиз запускают только собственные пути юнита
    (`publish_when`, иначе `paths`): общие входы пересобирают и тестируют юнит,
    но не публикуют его, иначе версия не растёт при изменившихся байтах. PR CI
    отдельно проверяет, что вычисленная версия не занята другим содержимым.
61. **Ручная пересборка — отдельный `deploy-services.yml`**, через те же
    reusable-пути и promote-контракт. Автодеплой работает только на байтах PR;
    пересборка — явное исключение (`replace_artifacts`), чтобы можно было
    исправить сломанный immutable-тег.
62. **Чат приложения — тот же Hermes через sessions API, история в шлюзе.**
    `mcp-gateway` `/v1/chat/*` ведёт одну сессию Hermes через relay
    `voice-proxy`: те же память и инструменты, без второго бота. История для
    телефона — SQLite шлюза; ходы строго по одному. Scheduler не подходит по
    домену, Hermes state.db не хранит метаданные приложения.
63. **Use case в Android — только с логикой.** Класс в
    `features/*/impl/domain` существует, если добавляет правило поверх
    репозитория; сквозные обёртки не заводятся — ViewModel берёт интерфейс
    репозитория напрямую.
64. **Данные Android-фичи — репозиторий над `EntityStore` в области фичи.**
    Репозиторий (`core/data` `EntitySource`) — единственный источник правды:
    `StateFlow` сущностей и состояния загрузки, мутации обновляют стор по
    ответу сервера (вкл/выкл и удаление — оптимистично с откатом), фоновая
    синхронизация не затирает локальные записи. Самодельные события
    «данные изменились» убраны: списки, редакторы и главный экран подписаны на
    стор. Репозитории живут в Metro graph extension фичи (`FeatureScope`),
    пока есть владельцы (ViewModel, сервис, toolbar-чип) и ещё 5 с после
    последнего; при уходе в фон пустая область очищается сразу. Экран может
    подключать секции других фич через их api (`ScreenSectionKey`) и
    флоу-ViewModel (`flowViewModel`), живущие пока флоу в back stack. Причина:
    один экземпляр данных на все экраны без вечных синглтонов и ручных
    перезагрузок.
65. **Android берёт данные из стора; сервер спрашивается только по сигналу.**
    Показ экрана и смена вкладки запросов не делают. Запрос — первая загрузка
    области фичи, pull-to-refresh, собственные мутации, `SyncSignal`:
    тихий FCM `type=sync` (`entity`, `op`, `id`, без содержимого), который
    scheduler шлёт через phone-push (`POST /v1/push/sync`) после каждой
    мутации и тика движка, кроме устройства из `Home-Device-Id`, и `Stale`
    при возврате приложения из фона дольше `StalenessPolicy` (5 мин).
    Сигнал идёт в репозитории (`core/data` `SyncSignals`), не в ViewModel и
    не через `AppEventBus`. Причина: лишние запросы на каждой вкладке и
    устаревшие данные при изменениях с других устройств и колонки.
66. **CI: параллельные шаги, один Bazel-вызов на job, полный отчёт.**
    Подготовка и независимые проверки идут фоновыми шагами GitHub Actions
    (`background`/`wait-all`); Bazel — один вызов `bazel test --keep_going`
    на job (параллельные вызовы на один output base блокируют друг друга).
    Android-сборка, тесты, Android Lint и detekt — один юнит `android-app`
    и один job: компиляция и подготовка один раз. Каждая проверка доходит до
    конца, job падает в конце со списком всех упавших (`step_report.mjs`,
    `bazel_report.mjs`). Причина: одна ошибка не прячет остальные, меньше
    runner-минут на повторную подготовку. Правила — `TESTING.md` (CI).
67. **История уведомлений — в scheduler, по одному deviceId.** Уведомление и
    запрос поиска телефона живут в той же SQLite, что реестр `push_devices`
    (схема v7): `notifications(id, device_id, type, title, body, deep_link,
    created_unix, read_unix)`, индекс `(device_id, created_unix DESC)`, не
    больше 200 записей на телефон, удаление устройства удаляет его историю.
    phone-push после успешной доставки пишет строку на каждый доставленный
    `deviceId`; `id` записи заранее кладётся в FCM `data`, поэтому повтор
    записи идемпотентен, а ошибка записи не ломает доставку. Чтение и
    мутации — plain JSON рядом с `/v1/push/devices` (`/v1/push/history`), а
    устройство берётся из `Home-Device-Id`, а не из тела. После изменения
    истории scheduler шлёт адресный sync-пуш `entity=notifications` только
    владельцу истории (`targetDeviceId`); если запрос пришёл с самого
    телефона, ему пуш не нужен — у него оптимистичное обновление. Причина:
    история —
    состояние домашнего сервера, а не телефона; `mcp-gateway` по решению 62
    владеет чатом Hermes, а не уведомлениями, и локальная DataStore-копия
    расходилась между устройствами и не переживала переустановку.
68. **Пагинация Android — один PagedList и чистый reducer.** Список с курсором
    собирается из `PagedSource`, `PagedList` (единственный владелец
    `StateFlow<PagedState>`) и чистого `PagingReducer` (`Refresh`, `Reload`,
    `LoadMore`, `Retry`), поэтому переходы тестируются без корутин. Правки —
    `ListEdit.Update/Remove/UpdateAll` через `commit` с откатом только своих
    ключей; скрытые ключи фильтруют текущий список и входящие страницы, без
    `pendingDeletes` в репозитории. Общие `PagingIntent`/`ListItemIntent`
    обрабатывает `PagingHandler`, `UndoableRemoval` держит undo поверх
    `UndoWindow`; экран описывает только свои интенты. `PagingBinder` одним
    вызовом связывает RecyclerView, `ConcatAdapter`, `ListStateViews`,
    SwipeRefresh и догрузку, а `SwipeActions` объявляет действия без ручного
    `ItemTouchHelper`. Причина: прежние `Pager`/`PagingDelegate` были тонкими
    обёртками, интенты и undo дублировались в каждом экране.
69. **Доступ приложения — модули, а не роли.** Источник правды — SQLite
    scheduler (schema v10): `app_tokens` (имя, sha256, модули, owner) и
    `access_meta.revision`; plaintext не хранится. Каждый app-эндпоинт
    принадлежит ровно одному модулю: `base` (`GetStatus`, `/v1/me`,
    регистрация push-устройства), `home` (`/v1/catalog`, `EnqueueTask`,
    `CancelTask`), `scenarios`, `alarms_timers`, `intercom_call`,
    `notifications`, `chat`, `vpn`, `system`, `infra_reports` (резерв),
    `alerts` (фильтр alert-сообщений и alert-push), `secrets`
    (`secrets-gateway /v1/secrets/*`), `admin` (`/v1/access/*`).
    Любой действующий токен получает `base`; legacy «Мама» временно —
    все модули, кроме owner-only `admin` и `secrets`, чтобы до UI-PR с
    именованными токенами не
    было регрессий у телефонов/HA/APK; сужение — отдельным решением после
    раздачи именованных токенов; owner —
    все модули, его нельзя отозвать или лишить `admin`. Проверка — общий
    крейт `home-app-auth` в каждом сервисе; ответ без модуля —
    `403 {"error":"module_forbidden","module":"…"}`. Остальные сервисы
    получают snapshot (`GET /v1/access/snapshot`, service-token): refresh 30 с,
    singleflight, fail-closed после 1 ч без успешного обновления;
    sync `entity=access` публикуется, обработка в Android — отдельным PR.
    Причина: один список модулей и одна проверка вместо ролей,
    копипасты и расхождений между сервисами; сервис-к-сервису остаётся на
    отдельном `service_token`, а не на app-токенах. Service-token обязателен
    везде, fallback на legacy нет; scheduler падает, если он совпадает с
    app-токеном. ESO отдаёт только явные ключи, и все они создаются в OpenBao
    до деплоя (`admin:service-token-init`); финализация ротации пишет `""`.
    Push-устройства и VPN-peer'ы привязаны к `token_id` (v9/v10), legacy-строки
    при старте назначаются «Маме», перехват push — тем же FCM-токеном или
    owner'ом, VPN — только owner'ом;
    `EnqueueTask/CancelTask` для `home` ограничены `control`/`home.control`.
70. **Секреты приложения — отдельный owner-only gateway поверх OpenBao.**
    Новый сервис `solutions/secrets-gateway` читает и меняет KV v2 через
    Kubernetes auth роль `secrets-gateway` с отдельным audience (не
    root/operator token). Владелец решил открыть из телефона весь каталог,
    кроме ядра: `openbao/*`, `infra/vpn/client-ca`, `k8s/flux-system/*`,
    `k8s/home-assistant/scheduler-auth`, `k8s/home-assistant/secrets-gateway`.
    Остальные пути, включая `infra/*` и `ci/*`, достижимы. Список задан в
    `manifests/deny.json`; из него генерируется `policy.hcl`, а сервис читает
    тот же файл из ConfigMap, поэтому код и policy не расходятся. Политика:
    read/update/delete на `secret/data/*` (delete — только soft-delete
    версии), read/list на `secret/metadata/*`; destroy, metadata-delete,
    undelete, sys/auth/identity запрещены. Модуль `secrets` — owner-only:
    scheduler store и admin API отказывают не-owner'у, owner имеет его неявно.
    Reveal требует 5-минутную сессию, открытую подписью ключа телефона;
    update/rollback/delete — подпись над точной операцией, CAS и запрет update
    soft-deleted версии (решение 71). Ошибки подписи и кода привязки дают
    экспоненциальный backoff и глобальный лимит; использованные nonce хранятся
    на PVC, поэтому рестарт не replay'ит подпись. API только soft-delete
    последней версии и CAS-запись; destroy отсутствует. Каждый доступ
    логируется без значений и уходит push'ем в phone-push с `module=secrets`
    (owner-устройства). Причина: телефон получает управляемые секреты без
    operator-токена и без расширения прав app-токенов; значения остаются в
    OpenBao, а UI — отдельный PR.
71. **Step-up секретов — ключ, привязанный к телефону.**
    TOTP заменён неэкспортируемым EC P-256 ключом в Android Keystore
    (StrongBox при наличии) с обязательной аутентификацией на каждое
    использование: `timeout 0`, `BIOMETRIC_STRONG | DEVICE_CREDENTIAL` на
    Android 11+, подпись через `BiometricPrompt.CryptoObject`. Привязка —
    одноразовый код из `admin:secrets-device-enroll` (15 минут, хранится
    хешем в OpenBao, single-use, constant-time, с backoff): приложение
    отправляет публичный ключ, цепочку key attestation и код, сервер хранит
    ключ и device id в `k8s/home-assistant/secrets-gateway-device`. Путь скрыт
    от HTTP API (`deny.json` + reserved path) и не синхронизируется ESO.
    Повторная привязка заменяет ключ, `--revoke` удаляет его, и сервис читает
    ключ на каждый запрос, поэтому отзыв действует без рестарта.
    Challenge — 32 случайных байта, привязанные к token id, действию,
    пути/ключу/версии и сроку 60 секунд; подпись ECDSA-P256 проверяется в Rust
    (`p256`), nonce одноразовый, использованные nonce пишутся на PVC.
    Reveal обменивает подпись `action=session` на 5-минутную сессию;
    update/rollback/delete подписывают точную операцию. Причина: код TOTP
    можно подсмотреть или переслать, а ключ не покидает Secure Element и
    каждая операция подтверждается биометрией или паролем устройства.
