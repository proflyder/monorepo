# Аудит архитектуры Android-приложения «Дом»

Срез: `origin/develop` на 2026-11-22 (после #375). Охват: `projects/mobile/android`
(`app`, `core/*`, `features/*/{api,impl}`) и `platform/android`. Параллельная
ветка `feat/vpn-network-observe` (реактивное автоподключение VPN) не учтена в
коде, но её зона покрыта находками V-*; при мердже пакета P4 сверить с ней.

Пути ниже — относительно `projects/mobile/android/`; `…/` сокращает
`src/main/kotlin/dev/proflyder/home/<module path>/`.

## Что уже хорошо

- Одна модель ошибки `Outcome<T>` + `RemoteFailure`, один маппинг в текст
  (`core/ui/ErrorText.kt`), один OkHttp/Retrofit-строитель, один DataStore и
  один шифр токена.
- `try`, `else if`, комментарии в `.kt`, сегментированные кнопки, жёсткие
  IP/хосты — не найдены. Все списки на `core/ui/BaseListAdapter`.
- Feature impl не зависят от чужих impl; навигация через `FeatureEntry` +
  `FragmentRouter` со StateFlow-состоянием хрома.

## 1. Целевая архитектура

### Слои и модули

```text
app                 композиция: Metro-граф, MainActivity (только хром), Set<AppStartable>
features/<f>/api    интерфейсы, доменные модели, Destination; без логики, без Android-SDK типов реализации
features/<f>/impl   internal: data (Repository impl, транспорты), domain (use case с логикой),
                    presentation (ViewModel + Fragment/Activity), DI-вклады
core/model          Outcome/RemoteFailure (отдельный Outcome.kt), Clock, DeviceIdentity, общие модели
core/network/api    контракты клиентов, RouteGate, Idempotent
core/network/impl   HomeHttp (rest/stream), интерсепторы, мапперы (internal), общий NetworkJson
core/settings       сторы на StateFlow со статусом Loading, без runBlocking
core/runtime (new)  @AppScope CoroutineScope, IoDispatcher, AppForeground: StateFlow<Boolean>,
                    tickerFlow, Backoff, AppStartable
core/ui             BaseViewModel (updateState, events), RefreshController, EditorDraftSession
core/designsystem   все визуальные компоненты, общие строки действий ds_action_*
```

Правила зависимостей:

1. `impl` видит только свой `api`, чужие `api` и `core/*`. `app` — единственный,
   кто видит все `impl`, но граф экспонирует **только API-типы**.
2. В `api` нет политики, протокольного кода, презентационных билдеров и
   стейт-машин — только контракты и модели.
3. Всё объявленное в `impl` — `internal` (кроме того, что требует Metro/манифест);
   правило проверяет detekt.
4. Никаких `object`-локаторов: Service/Activity/Receiver получают зависимости
   через Metro (аксессор графа / member injection).

### Шаблоны

- **Источник правды = StateFlow.** Repository/Store держит
  `StateFlow<T>` (через `stateIn(appScope, WhileSubscribed/ Eagerly)`);
  ViewModel только `combine` и `map`. Нет `load()`-снапшотов, нет обновления по
  `onResume`; периодика — `RefreshPolicy` внутри репозитория
  (`WhileSubscribed`).
- **Старт приложения.** `Set<AppStartable>` (`@ContributesIntoSet`):
  каждая фича вкладывает координатор, который *наблюдает* потоки в app-scope.
  `HomeApplication` только итерирует набор.
- **Окружение.** `NetworkEnvironment: StateFlow<HomeNetworkState>`
  (Unknown/Home/Away/Offline) на `callbackFlow` ConnectivityManager + проба;
  `AppForeground: StateFlow<Boolean>` из `ProcessLifecycleOwner`.
- **Транспорт за интерфейсом (VPN).**
  `VpnTransport { id; capabilities; prepare(): PermissionRequest?;
  up(TunnelRequest); down(); status: Flow<TransportStatus> }`,
  реализации `@ContributesIntoSet` в `impl/transport/<protocol>/`;
  `VpnTransportChain` — упорядоченный fallback. `VpnController` — актор с
  одной очередью команд. `AutoConnectCoordinator : AppStartable` =
  `combine(environment, settings, foreground, profile)` → команды контроллеру.
  `VpnProfile(deviceId, name, transports: Map<VpnProtocolId, TransportSettings>)`,
  каждая реализация сама декодирует свои настройки. Экраны видят только
  `VpnController.state` и `availableTransports`.
- **Push.** `PushTokenSource` (Firebase за интерфейсом) +
  `PushRegistrar : AppStartable` = `combine(token, connection.config,
  deviceName, appVersion).distinctUntilChanged()` + существующая
  due-политика; `HomeMessagingService` только пересылает события в app-scope.
- **Ошибка.** `Outcome<T>`/`RemoteFailure` в `core/model/Outcome.kt`;
  `RemoteFailure.Server(httpStatus: Int, code: ConnectCode)`,
  `Storage(reason: StorageReason)` — без строк; текст только в `ErrorText`.
- **Сетевой клиент.** Один `HomeHttp`: `rest(target): Retrofit` с suspend
  call adapter (без `io {}`-обёрток), `stream(target, decoder): Flow<T>` для
  SSE/Connect с общим `Backoff`. `Target` — sealed со своими
  `endpointOf(config)`, `connectProtocol`. Перед вызовом — suspend
  `RouteGate.await()` (без `runBlocking`/`CountDownLatch`). Повтор только для
  помеченных `@Idempotent`. Клиенты — `internal` + `@ContributesBinding`, без
  ручных `@Provides`. Один `NetworkJson`, мапперы — internal-расширения.
- **ViewModel.** `BaseViewModel<S>`: только `updateState {}` (detekt запрещает
  `mutableState.value =`), `events: Flow<E>` вместо `consume*`,
  `RefreshController` (latest-wins вместо `generation`), `EditorDraftSession<D>`
  для редакторов с `IdempotentRequestKey`. Фабрики через Metro
  `Provider`/assisted inject, без повторения списка аргументов.
- **Single responsibility и делегирование.** Крупные блоки одной логики
  (состояние экрана, загрузка/повторы, подписки, форма, таймеры) — отдельные
  классы, подключаемые делегированием Kotlin: `class X(...) : Y by impl`,
  `by lazy`, свойства-делегаты, свои `ReadOnlyProperty`/`ReadWriteProperty`
  для состояния и настроек. ViewModel/Fragment/адаптеры не копируют и не
  раздувают такую логику.
- **Однонаправленный поток UI.** Всё состояние UI, включая тумблеры, чекбоксы
  и выбранные табы, приходит из ViewModel через StateFlow. Фрагмент, адаптер и
  вью-холдер только шлют намерение (`onToggle(id, checked)`), ViewModel меняет
  стейт, UI рисует актуальное значение. Никаких `isChecked = !isChecked` и
  локальных флагов; биндинг без петли обратного вызова (снять слушатель →
  выставить значение → вернуть слушатель, или делегат-биндер). Проверяет
  detekt `NoUiOwnedControlState`.
- **Тикеры.** `tickerFlow(period)` в ViewModel, `Clock` инъектируется.

## 2. Находки

Серьёзность: **high** — нарушает целевую архитектуру или даёт баг/блокировку;
**med** — дублирование или утечка, мешающая расширению; **low** — стиль/гигиена.
Объём: S ≤ полдня, M 1–2 дня, L > 2 дней.

### DI, глобальное состояние, старт (D)

| id | сер. | модуль | путь:строка | проблема | целевое решение | объём |
|---|---|---|---|---|---|---|
| D1 | high | app, vpn, push, call | `app/…/HomeApplication.kt:21-48`; `features/vpn/impl/…/VpnEntryPoint.kt:7-25`; `features/push/impl/…/PushEntryPoint.kt:4-18`; `features/call/impl/…/CallEntryPoint.kt:5-22` | Глобальные `object`-локаторы с `@Volatile var dependencies`, заполняются вручную в `onCreate`; сервисы падают на `require()`; `install()` ещё и стартует работу | Metro-аксессор графа / member injection для Service/Activity; удалить `*EntryPoint` | M |
| D2 | high | app | `app/…/di/AppGraph.kt:44-46,69-88` | Граф экспонирует impl-типы (`OkHttpClient`, `TunnelVpnController`, `HomeNetworkMonitor`, `VpnOpenConnector`, `FirebasePushRegistrar`) | Экспонировать только API + `Set<AppStartable>` | M |
| D3 | high | app, core/network | `app/…/di/AppGraph.kt:135-243` | 8 копипастных `@Provides` клиентов с теми же 4–5 аргументами; push/VPN непоследовательно пропускают `networkWaiter` (`:209-229`) | `@ContributesBinding` на internal-реализациях + один `HomeHttp` | M |
| D4 | med | app, push, call | `app/…/HomeApplication.kt:30-31,49` | `pushRegistrar.start()`, `callNotifications.start()` — императивные вызовы на старте | `AppStartable`-вклады фич | S |
| D5 | med | core/settings, theme, remoteconfig | `core/settings/…/AndroidConnectionSettingsStore.kt:42`; `core/theme/…/AppearanceController.kt:25`; `core/remoteconfig/…/FirebaseRemoteConnectionSource.kt:35`; `core/settings/…/HomeDataStore.kt:22` | Каждый класс создаёт свой `CoroutineScope(SupervisorJob()+IO)` | Инъекция `@AppScope CoroutineScope` и `IoDispatcher` **Сделано в PR #379 (P1).** Скоупы и `IoDispatcher` инъектируются (`core/runtime/RuntimeBindings`). | S |
| D6 | med | core/ui | `core/ui/…/AppForeground.kt:3-17`; `app/…/MainActivity.kt:154` | Глобальный `@Volatile` синглтон, пишется из Activity | `AppForeground: StateFlow<Boolean>` из `ProcessLifecycleOwner` **Сделано в PR #379 (P1).** `AppForeground.state` из `ProcessLifecycleOwner` (`ProcessAppForeground : AppStartable`), грейс — `ForegroundGraceNoticePolicy`. | S |
| D7 | med | app | `app/…/di/NetworkInterceptors.kt:12-35` | `NetworkDiagnostics`-object; Chucker/логирование в main source set, переключение ресурсным bool | Debug-only binding container с `@Multibinds Set<Interceptor>` | S |
| D8 | med | all | `features/*/impl/…/*ViewModel.kt` (напр. `features/timers/impl/…/TimersViewModel.kt:183-199`) | 16 `ViewModelFactoryContributor` повторяют весь список аргументов конструктора | Metro `Provider`/assisted inject | M |
| D9 | low | app | `app/…/MainActivity.kt:60,91,205,247` | `(application as HomeApplication).graph` в четырёх местах | Инъекция Activity через граф | S |

### Разовые проверки вместо потоков (R)

| id | сер. | модуль | путь:строка | проблема | целевое решение | объём |
|---|---|---|---|---|---|---|
| R1 | high | vpn, app | `app/…/MainActivity.kt:146-156,171`; `features/vpn/impl/…/presentation/VpnActivityHost.kt:42-44`; `features/vpn/impl/…/tunnel/VpnOpenConnector.kt:35-45` | «Подключить при открытии» — проверка в `onStart` с повторной пробой сети | `AutoConnectCoordinator` на `combine(environment, settings, foreground, profile)`; Activity только запрашивает разрешение | M |
| R2 | high | vpn, core/network | `features/vpn/impl/…/tunnel/VpnOpenConnector.kt:58-61`; `app/…/di/HomeRouteNetworkWaiter.kt:9-12`; `AndroidNetworkWaiter.kt:13-24` | `NetworkWaiter` блокирует поток OkHttp через `runBlocking`/`CountDownLatch` до 20 с перед каждым запросом; дедлок на main | Suspend `RouteGate.await()` в suspend-пути клиента | M |
| R3 | high | core/settings, app | `core/settings/…/AndroidConnectionSettingsStore.kt:44-46`; `app/…/HomeApplication.kt:20` | `runBlocking` чтения DataStore и расшифровки токена на main в `Application.onCreate` | `stateIn(…, Loading)` с явным статусом загрузки; убрать прогрев | M |
| R4 | high | core/theme | `core/theme/…/AppearanceController.kt:50` | `runBlocking { dataStore.data.first() }` на main | Значение по умолчанию + поток / чтение под splash | S |
| R5 | high | app, core/network | `app/…/MainActivity.kt:149-153,319-323` | Activity сбрасывает `connectionPool.evictAll()` по магическим 15 с — UI знает об OkHttp | `ConnectionHealth` в core/network наблюдает foreground и сеть | S |
| R6 | med | push | `features/push/impl/…/FirebasePushRegistrar.kt:32-38,61-62` | Регистрация один раз в `start()`; настроили сервер позже — до холодного старта не регистрируется | `PushRegistrar` на `combine(token, config, name, version)` | M |
| R7 | med | push | `features/push/impl/…/HomeMessagingService.kt:12,21,26,58-60` | Scope сервиса отменяется в `onDestroy` сразу после колбэка — запись токена/истории может потеряться | Передать работу в app-scope регистратора | S |
| R8 | med | remoteconfig | `core/remoteconfig/…/FirebaseRemoteConnectionSource.kt:51-81,93-112` | Firebase-инициализация в `init`; `activateAndPublish`/`fetchAndPublish` дублируются, init-fetch без `refreshMutex` | Один `fetch(force)` под мьютексом, ленивый старт | S |
| R9 | med | alarms, reminders, timers, scenarios, control | `features/alarms/impl/…/AlarmsFragment.kt:91`; `RemindersFragment.kt:58`; `ScenariosFragment.kt:63`; `ControlFragment.kt:293`; `TimersFragment.kt:63-66` | Тихий refresh в `onResume` в пяти экранах | Репозиторий со StateFlow + `RefreshPolicy(WhileSubscribed)` **Для alarms/reminders/timers сделано в PR #PRN (P8):** обновление по `screenShown` (подписка на state, WhileSubscribed) и `AppEventBus` от редакторов; scenarios/control — P10/P11. | M |
| R10 | high | control | `features/control/impl/…/ControlViewModel.kt:306-318,555,559-565` | Состояние устройств подтверждается опросом: `reconcile` 8×1,5 с, settle 4,5 с после нажатия | `ControlRepository.catalog: StateFlow`, центральная политика обновления (позже — push-стрим HA) | L |
| R11 | med | control, scenarios | `ControlViewModel.kt:438`; `features/scenarios/impl/…/ScenariosViewModel.kt:52-73,118-126` | Избранное грузится снапшотом `favorites.load()` в двух VM и расходится | `FavoritePreferences.favorites: Flow` | S |
| R12 | med | call | `features/call/api/…/CallManager.kt:47-49` | `watch()`-стрим стартует в конструкторе app-синглтона, без управления жизненным циклом | `AppStartable` или `stateIn(WhileSubscribed)` | M |

### VPN (V)

| id | сер. | модуль | путь:строка | проблема | целевое решение | объём |
|---|---|---|---|---|---|---|
| V1 | high | vpn | `features/vpn/impl/…/tunnel/TunnelVpnController.kt:39-40,70-81,176-192` | Контроллер получает конкретные `WireGuardTransport` и `ExternalOpenConnectTransport`, отдельные `connect()`/`connectExternal()`, `when` по протоколу в `alive`/`fallback`/`stop` — новый протокол = правка контроллера | `VpnTransport` + `Set`/`VpnTransportChain`; жизнеспособность и fallback — в транспорте | L |
| V2 | high | vpn | `features/vpn/impl/…/presentation/VpnViewModel.kt:18-20,46-51,77` | ViewModel импортирует `TunnelVpnController`, `HomeNetworkMonitor`, `ExternalOpenConnectTransport`, вызывает `openExternal()` | Только API-интерфейсы; fallback как данные `availableTransports` | M |
| V3 | high | vpn/api | `features/vpn/api/…/VpnRules.kt:30-57,139-183` | В API — рендер WireGuard-конфига, парсинг UAPI-статистики, сборка `anyconnect://` | В `impl/transport/{wireguard,openconnect}` как internal | M |
| V4 | high | vpn/api | `features/vpn/api/…/VpnRules.kt:59-120,185-212` | Политики (`HandshakePolicy`, `HomeNetworkRules`, `OpenConnectRules` — на деле «connect on open» для любого протокола) в API | `internal AutoConnectPolicy`/`ConnectOnOpenPolicy` в impl, тесты туда же | M |
| V5 | high | core/model | `core/model/…/VpnModels.kt:22-26` | `VpnProfile` с полями `wireguard` и `ocserv` — общий модуль протокол-специфичен | `transports: Map<VpnProtocolId, TransportSettings>` | L |
| V6 | high | vpn | `features/vpn/impl/…/tunnel/WireGuardTransport.kt:40-45,53`; `HomeNetworkMonitor.kt:77` | «Дома» проталкивается в транспорт как `@Volatile var atHome`, гонка с `up()` | `TunnelRequest(route, atHome)` на каждый `up()` или чтение `NetworkEnvironment.state` | S |
| V7 | high | vpn | `features/vpn/impl/…/tunnel/HomeNetworkMonitor.kt:30-35,95-105` | Монитор одновременно детектирует сеть и решает авто-подключение/отключение | Разделить: `NetworkEnvironment` (только StateFlow) и `AutoConnectCoordinator` | M |
| V8 | med | vpn | `features/vpn/impl/…/tunnel/TunnelVpnController.kt:52-68` | Методы вне интерфейса (`start`, `awaitLoaded`, `hasProfile`, `permissionIntent`, `connectExternal`, `restartIfActive`, `followNetwork`, `homeChanged`) используются снаружи в обход `VpnController` | Нужное — в API (`permissionRequest()`), прочее — internal-коллабораторы | M |
| V9 | med | vpn | `TunnelVpnController.kt:44,70-75,83-86` | Неволатильный `job` меняют UI, тайл, сервис, монитор; `disconnect()` гоняется с новым `connect()` | Актор (`Channel`) или `Mutex`, одна корутина | M |
| V10 | med | vpn | `TunnelVpnController.kt:150-174` | Опрос рукопожатия 500 мс и трафика 1 с работает без наблюдателей | `status: Flow` транспорта, статистика только при подписчиках | M |
| V11 | med | vpn | `features/vpn/impl/…/tunnel/ExternalOpenConnectTransport.kt:41-44` | Рукопожатие подделывается `currentTimeMillis()` при любом системном VPN — чужой VPN = «подключено» | `TRANSPORT_VPN`-callbackFlow, честный статус «внешний, не проверен» | S |
| V12 | med | vpn | `features/vpn/impl/…/tunnel/WireGuardTransport.kt:77-84,96-97` | Рефлексия по приватным полям `GoBackend` — сломается молча при обновлении | Публичный `backend.getStatistics()` или адаптер с тестом | S |
| V13 | med | vpn, push | `features/vpn/impl/…/data/RemoteVpnProfiles.kt:35` | VPN зависит от push-овского `PushDeviceIdentity` | `DeviceIdentity` в core **Сделано в PR #379 (P1).** `DeviceIdentity` в core/model, реализация `AndroidDeviceIdentity` в core/runtime. | S |
| V14 | med | vpn/api, core/network | `features/vpn/api/…/VpnRules.kt:121-137`; `core/network/…/HttpSupport.kt:73` | Маппинг ошибок по строкам `"http_400"` ↔ `"http_$httpCode"` | `RemoteFailure.Server.httpStatus: Int` **Сделано в PR #379 (P1).** `RemoteFailure.Server.httpStatus: Int`, `VpnProblems` по числу. | S |
| V15 | med | vpn | `features/vpn/impl/…/VpnEntryPoint.kt:18-22` | `ConnectivityManager`-колбэк монитора никогда не снимается | `callbackFlow` с `awaitClose` | S |

### Сетевой слой (N)

| id | сер. | модуль | путь:строка | проблема | целевое решение | объём |
|---|---|---|---|---|---|---|
| N1 | high | core/network | `core/network/…/Retrofits.kt:179-210`; `ChatClient.kt:267-273`; `ConnectIntercomClient.kt:62-67` | Каждый клиент строит свой Retrofit; два ручных стрим-клиента дублируют redirect/retry/интерсепторы, intercom без диагностики | `HomeHttp.rest(target)` / `stream(target, decoder)` | M |
| N2 | med | core/network | `core/network/…/api` vs impl (весь модуль) | Контракты и реализации в одном модуле, реализации public | Разделить `core/network/{api,impl}`, impl internal | M |
| N3 | med | core/network | `ConnectScenarioClient.kt:78`; `ConnectSchedulerClient.kt:163`; `ConnectIntercomClient.kt:182`; `ChatClient.kt:276-297`; `features/push/impl/…/PushDevices.kt:93-117` | Блокирующий call adapter ⇒ `io {}`/`withContext` во всех клиентах | Suspend-native `OutcomeCallAdapterFactory` | M |
| N4 | med | core/network, chat | `ChatClient.kt:300-305`; `ConnectIntercomClient.kt:140-176`; `features/chat/impl/…/data/ChatRepository.kt:40-41,137-167` | Дважды цикл execute→read с разной обработкой ошибок; 3 ручных backoff 1→30 с | `StreamingCall` → `Flow` + общий `Backoff` | M |
| N5 | med | core/network | `HttpSupport.kt:52-57` vs `Retrofits.kt:162-166` | Ограничение размера ответа реализовано дважды | Одна реализация | S |
| N6 | med | core/network | `Retrofits.kt:180`; `ChatClient.kt:331`; `ConnectIntercomClient.kt:185` | `PLACEHOLDER_BASE_URL` трижды, URL склеиваются строками | Константа в `HomeHttp`, `@Streaming`-методы | S |
| N7 | med | core/network | `ReadRetry.kt:48,63-66` | `Thread.sleep`, `evictAll` на каждый повтор, идемпотентность угадывается по префиксу `List`/`Get` | `@Idempotent`-тег, suspend-задержка | S |
| N8 | med | core/network | `ConnectionInterceptor.kt:23-38,58` | Маппинг `Target` дважды, `if (target == …)` для Connect-заголовка | `Target` — sealed со своими свойствами | S |
| N9 | med | core/network | `WireDtos.kt:36`; `ControlCatalogClient.kt:28`; `IntercomJson.kt:17`; `SchedulerJson.kt:14`; `ScenarioJson.kt:17` | 5 конфигураций `Json` в одном модуле | Один `NetworkJson` | S |
| N10 | low | core/network | `SchedulerJson.kt`, `ScenarioJson.kt:108-152`, `IntercomJson.kt`, `ControlCatalogClient.kt` (~1100 строк) | Ручные public-мапперы и `buildJsonObject` | internal-расширения, `@Serializable`; позже — codegen из protobuf | L |
| N11 | low | core/network | `ConnectionInterceptor.kt:77` | `USER_AGENT "home-android/0.1.0"` не совпадает с версией | Инъекция `VersionInfo` | S |
| N12 | low | core/network | `HttpSupport.kt:59` | `parseConnectFailure` public, коды строками | internal + enum `ConnectCode` | S |
| N13 | low | chat, core/network | `ChatRepository.kt:46` (30 с) vs `ReadRetry.kt:57` (3 с) | Два разных таймаута «ждать сеть» | Один параметр `RouteGate` | S |

### Настройки и конфиг (C)

| id | сер. | модуль | путь:строка | проблема | целевое решение | объём |
|---|---|---|---|---|---|---|
| C1 | med | core/config | `core/config/…/RemoteConnectionSource.kt:37-115` | Ручной разбор JSON-дерева, блоки control/intercom копипастой | `@Serializable` DTO + `parseOptionalEndpoint` | S |
| C2 | med | core/config | `core/config/…/ConnectionConfig.kt:22-45` | Валидация цепочкой `if`-return | Список правил / `when` | S |
| C3 | med | core/settings | `AndroidConnectionSettingsStore.kt:97,109` | Английский пользовательский текст в `RemoteFailure.Storage(...)` | `Storage(reason: enum)`, строки в ресурсах **Сделано в PR #379 (P1).** `Storage(reason: StorageReason)`, тексты в ресурсах `core/ui`. | S |
| C4 | low | core/settings | `AndroidConnectionSettingsStore.kt:143-146` | Мёртвые ключи `KEY_ENDPOINT`, `KEY_*_TIMEOUT` | Удалить или оставить только в миграции | S |
| C5 | low | core/settings | `AndroidConnectionSettingsStore.kt:81-93` | `updateStoredToken` с одним вызовом + `if/else` | Встроить | S |
| C6 | med | vpn, push, scenarios, core/config | `StoredVpnProfile.kt:42`; `NotificationHistoryCodec.kt:18`; `ScenarioEditorDraft.kt:42`; `RemoteConnectionSource.kt:37` | Ещё 4 локальных `Json { ignoreUnknownKeys = true }` | Общий `LocalJson` | S |
| C7 | med | chat | `features/chat/impl/…/data/ChatOutboxStore.kt:90-124` | Ручной `org.json.JSONObject` рядом с kotlinx.serialization | `@Serializable` + общий Json | M |
| C8 | low | connection | `features/connection/impl/…/ConnectionViewModel.kt:79,83` | Ручной rethrow + английская строка «Remote Config could not be refreshed.» | `RemoteFailure.RemoteConfig` + ресурс | S |
| C9 | low | connection | `ConnectionViewModel.kt:94-149` | `persist` — вложенность ~5 | Разбить на `saveThenMaybeTest`/`applyTestResult` | S |

### Push (P)

| id | сер. | модуль | путь:строка | проблема | целевое решение | объём |
|---|---|---|---|---|---|---|
| P1 | med | push | `features/push/impl/…/FirebasePushRegistrar.kt:44-56,69-87,113` | Прямые `FirebaseMessaging.getInstance()`/`FirebaseApp`; флаги busy/failed вручную, не восстанавливаются при исключении | `PushTokenSource` за интерфейсом; busy из in-flight job | M |
| P2 | med | push, vpn | `features/push/impl/…/PushNotifier.kt:118-127`; `features/vpn/impl/…/tunnel/VpnStatusService.kt:76-79`; `PushPermissions.kt:30,45` | Проверка `POST_NOTIFICATIONS` трижды, две — `if/else` | `NotificationPermission` в core/ui | S |

### Звонок (K)

| id | сер. | модуль | путь:строка | проблема | целевое решение | объём |
|---|---|---|---|---|---|---|
| K1 | high | call/api | `features/call/api/…/CallManager.kt:25-344` | Вся стейт-машина звонка (сигналинг, watchdog, media jobs, `scope.launch` в `init`) в API | В impl как internal; в API — `CallController`, `CallSnapshot` | M |
| K2 | high | call | `features/call/impl/…/presentation/CallActivity.kt:114-117` | `onDestroy` всегда `audioController.stop()` — поворот экрана глушит идущий звонок | Аудиомаршрут привязать к состоянию звонка/сервису | S |
| K3 | high | call | `CallActivity.kt:40-336` | Нет ViewModel: разрешения, `pendingStart`, wakelock, таймер, маппинг причин в Activity | `internal CallViewModel` со StateFlow | M |
| K4 | med | call | `CallActivity.kt:208-225` | Ручной таймер `while(true){delay(1000)}` + `String.format("%02d:%02d")`; FQ-имена `IntercomEndReason` | `tickerFlow` + `DateUtils.formatElapsedTime`; `CallTexts` | S |
| K5 | med | call | `features/call/impl/…/CallForegroundService.kt:22,38-47` | Свой `CoroutineScope(Main)`, вложенные `if` | `LifecycleService` + `when` по фазе | S |
| K6 | med | call | `features/call/impl/…/AppCallController.kt:13-21` | Прокладка только из-за `CallManager` в API | Убрать после K1 | S |

### Управление домом, сценарии, чат (F)

| id | сер. | модуль | путь:строка | проблема | целевое решение | объём |
|---|---|---|---|---|---|---|
| F1 | high | control | `features/control/impl/…/ControlAdapter.kt:187-1009` | God-адаптер на 1009 строк, 8 вложенных ViewHolder, ручной diff/payload | Делегаты по типу элемента, holder на файл | L |
| F2 | high | control | `ControlAdapter.kt:645-689` vs `960-994`; `369-389` vs `695-740` | `bindVolumeRow`/`bindButtonRow`/`bindTitle`/`bindBusy`/… скопированы между holder'ами | `DeviceRowBinder`, `EntryRowBinder` | M |
| F3 | high | control | `features/control/impl/…/ControlViewModel.kt:86-560` | God-VM 625 строк: каталог, хаб, избранное, оптимистичные ожидания, громкость, reconcile | `HubStateHolder`, `OptimisticDeviceState`, `FavoritesStore`; VM только combine | L |
| F4 | med | control | `ControlAdapter.kt:491-503,806-840` | Строки сетки строятся кодом (`LinearLayout`, спейсеры, `while(childCount…)`) | Компонент сетки плиток в designsystem | M |
| F5 | med | control/api | `features/control/api/…/ControlItems.kt:116-347`; `HubCards.kt:37-72`; `SpeedDial.kt:15`; `WakeFeedback.kt:18` | Презентационные билдеры в API, используются только control/impl | В impl, internal | M |
| F6 | med | control/api | `features/control/api/…/QuickTiles.kt:9-30` | Sealed `QuickTile` жёстко знает `Call` и `Vpn` | `QuickTileProvider`-мультибиндинг от фич | M |
| F7 | med | control, timers, call | `ControlFragment.kt:206-211`; `features/timers/impl/…/TimersFragment.kt:55-60`; `CallActivity.kt:209` | Три тикера `while(true){delay(1000)}` в UI | `tickerFlow` в core/runtime, сбор в VM | S |
| F8 | med | control | `ControlAdapter.kt:244,653,668` | `0f/180f`, повторяющееся `if (available) 1f else DISABLED_ALPHA` | `View.bindAvailability` в designsystem | S |
| F9 | high | chat | `features/chat/impl/…/data/ChatRepository.kt:73-78` | Конкретный public `@SingleIn` репозиторий без интерфейса; VM зависит от `ChatRepository`/`ChatMedia` напрямую | Интерфейс + internal impl `@ContributesBinding` | M |
| F10 | med | chat | `ChatRepository.kt:137-145`; `features/chat/impl/…/presentation/ChatFragment.kt:93` | Живой стрим — ручной `while(true)` backoff, запускается Fragment'ом (`repeatOnView { runLive() }`) | `events: Flow` с `retryWhen`, `stateIn(WhileSubscribed)` в VM | M |
| F11 | med | chat | `ChatFragment.kt:53-57,185-196` | Fragment владеет `VoiceRecorder`/плеером, опрашивает `recording` циклом | `RecordingState` StateFlow в компоненте/VM | M |
| F12 | low | chat | `features/chat/impl/…/presentation/ChatViewModel.kt:97-105` | VM оперирует `Uri` и `File` | `ChatMedia` с непрозрачным источником вложения | S |
| F13 | med | scenarios | `features/scenarios/impl/…/presentation/{Delay,Speak,EntityCondition,TimeTrigger,TimeWindow,LightParams}Dialog.kt` | 6 одинаковых `MaterialAlertDialogBuilder`-диалогов, не DialogFragment — теряют состояние при повороте | Общий `EditorDialogFragment<T>` в designsystem | M |
| F14 | med | scenarios, control | `LightParamsDialog.kt:14-38,60,87-88` vs `features/control/impl/…/LightSheet.kt:26,67-69` | Логика яркость/кельвины продублирована с разными границами; сценарий правит сырой JSON-строкой (`"null"`) | Общий `LightParamsEditor` + типизированный `LightParams` | M |
| F15 | med | scenarios | `features/scenarios/impl/…/ScenarioEditorViewModel.kt:59-92,220-283` | VM 422 строки, сама ведёт пару idempotency-key/endpoint через `ConnectionSettingsStore` | В use case `SaveScenario` / `EditorDraftSession` | M |
| F16 | med | scenarios/api | `features/scenarios/api/…/ScenarioEditorDraft.kt:40` | UI-черновик и SavedState-кодек в API | В impl, internal | S |
| F17 | med | core/model | `core/model/…/Favorites.kt:14` | Кириллические/английские метки (`"блэкаут"`, `"сел работать"`) в модели | Ресурсы или удалённый конфиг | S |
| F18 | low | system | `features/system/impl/…/SystemFragments.kt:44-49` | Polling во Fragment | `RefreshPolicy` в репозитории | S |

### Время: будильники, напоминания, таймеры (T)

| id | сер. | модуль | путь:строка | проблема | целевое решение | объём |
|---|---|---|---|---|---|---|
| T1 | high | timers | `features/timers/impl/…/TimersViewModel.kt:65-174` | 15 прямых `mutableState.value = …copy()` — неатомарный read-modify-write | `updateState {}` **Сделано в PR #PRN (P8).** `TimersViewModel` на `StateViewModel`, состояние меняется только через `updateState`. | S |
| T2 | high | reminders | `features/reminders/impl/…/ReminderEditorViewModel.kt:97-189` (`:135`) | 21 прямая запись; `save()` пишет `current.copy` и теряет параллельные правки | `updateState {}` **Сделано в PR #PRN (P8).** `save()` берёт снимок формы из `currentState` и пишет только через `updateState`; общий `EditorDraftSession`. | S |
| T3 | med | alarms, scenarios, remoteconfig, vpn, call, theme | напр. `features/alarms/impl/…/AlarmEditorViewModel.kt:107` | Ещё ~57 прямых записей состояния | `updateState` + detekt-правило **Сделано в PR #PRN (P8)** для alarms (в т.ч. шит повторов). Остальные модули — в своих пакетах (P5/P6, P9, P10, P11, P4). | M |
| T4 | med | alarms, timers, reminders, scenarios, control | `AlarmsViewModel.kt:41-63`; `TimersViewModel.kt:48-82` | Логика «игнорировать устаревший refresh» (`refreshJob`, `++generation`, `refreshing \|\| !silent`) скопирована 5 раз | `RefreshController` в core/ui **Сделано в PR #PRN (P8).** `ListRefresher` на `LatestLauncher` (последний запрос побеждает); `refreshJob`/`generation` удалены в alarms/reminders/timers. | M |
| T5 | med | 5 списковых экранов | поля `refreshing`, `loadError`, `actionError`, `busy` + `consume*` (10 копий) | Повтор загрузочного состояния и одноразовых событий | `ListLoadState` + `events` в `BaseViewModel` **Сделано в PR #PRN (P8)** для alarms/reminders/timers: `ListLoadState` + `ExclusiveActions`, ошибки — события `StateViewModel.events`, `consume*` удалены. | M |
| T6 | med | alarms, reminders | `AlarmEditorViewModel.kt:72-94` vs `ReminderEditorViewModel.kt:63-85,193-220` | Почти идентичные draft + `IdempotentRequestKey` + `adoptRestoredCreateIdentity` + `persistDraft` | `EditorDraftSession<D>` **Сделано в PR #PRN (P8).** `EditorDraftSession<D, P, C>` + `EditorDraftCodec` + `EditorStatus` в `core/presentation`. | M |
| T7 | med | timers | `TimersViewModel.kt:50,147-157` | `System.nanoTime()` в VM, тик пересоздаёт всё состояние каждую секунду | Инъекция `Clock`, обратный отсчёт отдельным потоком **Сделано в PR #PRN (P8).** `Clock` инъектируется (`TimerRefreshPolicy`), обратный отсчёт — `tickerFlow` только пока экран подписан; тик пересчитывает `rows`, адаптер обновляет лишь `REMAINING`. | M |
| T8 | low | timers | `TimersFragment.kt:41-46` | Пресеты `"1m"/"5m"/…` строками с обратным парсингом | Список `Duration` **Сделано в PR #PRN (P8).** `TIMER_PRESETS: List<Duration>` + `Duration.toTimerInput()` в `timers/api`. | S |
| T9 | low | timers, control | `features/timers/impl/…/ConnectTimerRepository.kt:20-23`; `ControlViewModel.kt:156` | `syncClock` в таймерном репозитории, но зовёт и Control | Сервис серверных часов в core **Сделано в PR #379 (P1).** `ServerClockSync` (core/model) + `SchedulerServerClockSync` (core/network). | S |
| T10 | low | alarms, reminders, timers | `features/alarms/impl/…/AlarmUseCases.kt:11-21` и аналоги | Сквозные use case без логики | Решение в `DECISIONS.md`: оставлять только с логикой **Сделано в PR #PRN (P8).** Решение 63 в `DECISIONS.md`; сквозные use case alarms/reminders/timers удалены, остались `SaveAlarm`, `SnoozeAlarm`, `SaveReminder`, `LoadTimers`, `StartTimer`. | S |
| T11 | low | alarms | `AlarmsViewModel.kt:53` | Сортировка будильников в VM; у таймеров/напоминаний своя | Доменный компаратор в API **Сделано в PR #PRN (P8).** `AlarmOrder`, `ReminderOrder`, `TimerOrder` в API; у напоминаний исправлена строковая сортировка (09 vs 10). | S |
| T12 | med | alarms, reminders, timers | `AlarmsFeatureEntry.kt:14,19,36`; `RemindersFeatureEntry.kt:14,19,37`; `TimersFeatureEntry.kt:12,19` | Destination экранов public в impl | В API или internal **Сделано в PR #PRN (P8).** Destination и фрагменты — `internal` в impl (в API нельзя: `Destination.arguments()` — Android `Bundle`, а API — JVM-модули); app создаёт секции «Времени» через `FeatureEntry.createFragment`. | M |
| T13 | low | app | `app/…/TimeFeatureEntry.kt`, `TimeFragment.kt` | Фича «Время» живёт в app | `features/time/{api,impl}` | S |

### Навигация и хром (U)

| id | сер. | модуль | путь:строка | проблема | целевое решение | объём |
|---|---|---|---|---|---|---|
| U1 | med | app | `app/…/MainActivity.kt:52-321` (`120-127`, `181-192`, `271-281`, `321`) | God-Activity: чип профиля через `ControlViewModel`, `CONTROL_TAB = "control"`, разбор push deep-link, VPN-хост, меню | `ToolbarContribution`-набор от фич, `DeepLinkResolver` в core/navigation | M |
| U2 | med | app, core/navigation | `MainActivity.kt:114-119` | Два коллектора `currentTabId` и ручное `.value` | Один `AppChromeState` StateFlow | S |
| U3 | med | core/navigation | `core/navigation/…/Destination.kt:8-42`; `FragmentRouter.kt:112` | Маршруты — строки (`startsWith("$id/")`), аргументы — сырой `Bundle` | Типизированный парсер маршрута на `FeatureEntry`, Parcelable-аргументы | M |
| U4 | low | core/model | `core/model/…/SchedulerModels.kt:109,162` | `Outcome`/`RemoteFailure` в scheduler-файле | `core/model/Outcome.kt` **Сделано в PR #379 (P1).** `core/model/Outcome.kt`. | S |

### Строки и ресурсы (S)

| id | сер. | модуль | путь:строка | проблема | целевое решение | объём |
|---|---|---|---|---|---|---|
| S1 | med | все `res/values*` | «Delete» ×8, «Cancel»/«Retry»/«Pause» ×4, «Open Connection» ×3 | Общие действия определены в каждом модуле | `ds_action_*` в core/designsystem | M |
| S2 | low | app, features | «Home» ×5, «Timers»/«Reminders» ×3 | Заголовки экранов дублируются | Один источник на фичу | S |

### Правила Kotlin проекта (Q)

| id | сер. | модуль | путь:строка | проблема | целевое решение | объём |
|---|---|---|---|---|---|---|
| Q1 | med | все impl | push 28, vpn 24, settings 14, call 13, scenarios 13, alarms 12, reminders 12, chat 11, system 11, control 9, timers 8, connection 8 | 163 public top-level объявления в `impl` при правиле `internal` | Сделать internal после D2; detekt-правило `PublicInImpl` (правило добавлено в PR P0, по нему в baseline 197 объявлений: считаются все top-level, включая функции, свойства и typealias) | M |
| Q2 | med | весь проект | control/impl 43, chat/impl 27, scenarios/impl 16, alarms/impl 15, core/ui 13; напр. `core/network/…/HttpSupport.kt:36`, `OutcomeCallAdapterFactory.kt:82-86`, `features/control/api/…/LightColors.kt:9-10`, `ConnectionFragment.kt:123,140`, `FindPhoneActivity.kt:45` | ~~172 `if … else` при правиле «только when»~~ — **неверная находка** (поправка владельца): запрещён только `else if` (третья ветка), обычный `if`/`else` разрешён; `else if` уже ловит `NoElseIf`, нарушений нет | Ничего не делать; **закрыто в PR P0** | — |
| Q4 | low | control | `features/control/impl/…/presentation/SpeedDialOverlay.kt:54` | Открыто ли меню быстрых действий хранит локальный `isOpen` в UI, а не стейт ViewModel (найдено при вводе `NoUiOwnedControlState`, правило такой флаг не ловит) | Состояние меню в `ControlViewModel`, оверлей рисует его | S |
| Q3 | low | detekt | `platform/android/detekt-rules/`, `detekt.yml:81-91` | Не проверяются: `internal` в impl, `mutableState.value =` (else-if уже проверяет `NoElseIf`) | Добавить два правила | M — **сделано в PR P0**: `PublicInImpl`, `NoDirectStateWrite` + `projects/mobile/android/detekt-baseline.xml` (197 + 59 записей); плюс `NoUiOwnedControlState` (однонаправленный поток UI) — нарушений нет, без baseline |

Соблюдены: `runCatching` вместо `try` (0 `try`), лямбды ≤3 строк и вложенность ≤3
(detekt `LongLambda`, `NestedBlockDepth`; исключение — C9), строки из ресурсов
(кроме C3, C8, F17), сегментированных кнопок нет, комментариев в `.kt` нет.

### Сводка

| серьёзность | число |
|---|---|
| high | 26 |
| med | 61 |
| low | 19 |
| всего | 106 |

## 3. План исправления

Каждый пакет — отдельный PR в `develop` с зелёным `ci-ok`, мерджится
независимо; порядок — от фундамента к миграции. Общий критерий для всех:
`bazel build //projects/mobile/android/...`, `bazel test` затронутых модулей,
`android-lint`, `android-detekt`, `mise run lint:comments`, скриншот-тесты
без незапланированных изменений, обновлены README модулей («Заметки к коду»).

| # | пакет | находки | критерий готовности |
|---|---|---|---|
| P0 | **Правила и ограждения**: detekt `PublicInImpl`, `NoDirectStateWrite` (сначала в режиме baseline); else-if — существующий `NoElseIf` — **сделано в PR P0** | Q3, Q2 (неверная) | Правила в `detekt-rules` с тестами; baseline фиксирует текущие нарушения, новые — красный CI |
| P1 | **core/runtime** (сделано в PR #379): `@AppScope` scope, `IoDispatcher`, `AppForeground` StateFlow, `AppStartable`, `tickerFlow`, `Backoff`, `Clock`, `DeviceIdentity`; `Outcome` в свой файл, `RemoteFailure.Server.httpStatus`, `Storage(reason)` | D5, D6, U4, V13, V14, C3, T9 | Нет `CoroutineScope(SupervisorJob…)` в core; `HomeApplication` итерирует `Set<AppStartable>`; тесты `tickerFlow`/`Backoff` |
| P2 | **DI без локаторов**: Metro-аксессоры для Service/Activity, удаление `*EntryPoint`, граф экспонирует только API, VM-фабрики через Provider/assisted, debug-интерсепторы через мультибиндинг | D1, D2, D4, D7, D8, D9 | Нет `object *EntryPoint`; `AppGraph` без impl-типов; push/call стартуют как `AppStartable` |
| P3 | **Сеть**: `core/network/{api,impl}`, `HomeHttp.rest/stream`, suspend call adapter, `RouteGate`, `@Idempotent`, sealed `Target`, `NetworkJson`, `ConnectionHealth`, internal-мапперы | D3, R2, R5, N1–N9, N11–N13, C6 | Нет `runBlocking`/`CountDownLatch`/`Thread.sleep` в сети; один Retrofit-фабричный путь; `MainActivity` не знает OkHttp; существующие тесты клиентов зелёные |
| P4 | **Неблокирующий старт настроек**: стор и тема на `stateIn(Loading)`, Remote Config одним `fetch(force)`, `@Serializable` разбор конфига | R3, R4, R8, C1, C2, C4, C5 | Нет `runBlocking` в `core/settings` и `core/theme`; StrictMode-тест старта без disk read на main |
| P5 | **VPN: окружение и координатор** (сверить/слить с `feat/vpn-network-observe`): `NetworkEnvironment`, `AutoConnectCoordinator`, политики в impl, `TunnelRequest` вместо `atHome` | R1, V4, V6, V7, V15 | Activity не вызывает `onForeground`/`checkNow`; автоподключение покрыто тестом на фейковых потоках |
| P6 | **VPN: транспорт за интерфейсом**: `VpnTransport` + `VpnTransportChain`, протокол-нейтральный `VpnProfile`, протокольный код в `impl/transport/*`, актор-контроллер, статус потоком, честный статус внешнего VPN | V1, V2, V3, V5, V8–V12 | Добавление фейкового третьего транспорта в тесте не требует правок VM/Fragment/контроллера; в `vpn/api` нет WireGuard/OpenConnect |
| P7 | **Push реактивно**: `PushTokenSource`, `PushRegistrar` на `combine`, работа сервиса в app-scope, `NotificationPermission` | R6, R7, P1, P2 | Смена сервера/токена регистрирует без перезапуска (тест на фейках); нет `FirebaseMessaging.getInstance()` вне адаптера |
| P8 | **Базовый ViewModel-слой** на `core/presentation`: `StateViewModel<S, I, E>` (`updateState`-only, `events`), `by latest()` вместо `generation`/`RefreshController`, `RefreshMode`, `ListLoadState`, `EditorDraftSession`; `AppEventBus` (`ScheduleChanged` из редакторов → мягкое обновление списков); миграция alarms/reminders/timers — **сделано в PR #PRN** | T1–T8, T10–T12, R9 (для time) | Baseline `NoDirectStateWrite` для этих модулей пуст; нет `generation`/`consume*`; refresh не в `onResume` |
| P9 | **Звонок**: `CallManager` в impl, `CallViewModel` на `StateViewModel` (команды — намерения, «положить трубку»/разрешения — события), аудио по состоянию звонка, `LifecycleService` | K1–K6, R12 | Поворот во время звонка не глушит звук (Robolectric-тест); в `call/api` только контракты |
| P10 | **Управление домом**: репозиторий каталога со StateFlow и политикой обновления, разбиение VM и адаптера (`KeyedDiff` + `aspectPayload`, клики через `IntentAdapterCallback`, тумблеры устройств только из state), `QuickTileProvider`, билдеры в impl, общий `FavoritePreferences` поток | R10, R11, F1–F8, R9 (control) | `ControlAdapter`/`ControlViewModel` < 300 строк каждый; нет `delay`-reconcile в VM; плитки VPN/звонка вкладываются своими фичами |
| P11 | **Сценарии и чат**: `EditorDialogFragment<T>`, `LightParamsEditor` + `LightParams`, draft в impl, редакторы на `StateViewModel` + `SwitchBinder`; интерфейс `ChatRepository`, стрим потоком, `RecordingState`, `@Serializable` outbox | F9–F16, C7, R9 (scenarios) | Диалоги переживают поворот; `ChatFragment` не запускает стрим; общий редактор света в designsystem |
| P12 | **Хром и навигация**: `ToolbarContribution`, `DeepLinkResolver`, `AppChromeState`, типизированные маршруты (переход — событие `OpenRoute` из ViewModel), `features/time`, system polling в репозиторий; все экраны — `ScreenFragment` | U1–U3, T13, F18 | `MainActivity` < 150 строк, без `CONTROL_TAB`; тесты `FragmentRouter` на типизированных маршрутах |
| P13 | **Строки и финальная чистка**: `ds_action_*`, заголовки, `Favorites` из ресурсов, конфиги без английских строк, снятие baseline'ов detekt | S1, S2, F17, C8, C9, Q1 | Baseline'ы detekt удалены, правила P0 действуют без исключений |

Зависимости: P0 → все; P1 → P2, P5, P7, P8; P3 после P2; P6 после P5;
P8–P11 после P1/P2 и между собой независимы; P13 последним.

## 4. Базовый слой presentation

Модуль `core/presentation` задаёт общий каркас экранов. Он надстроен над
`core/ui` (`BaseViewModel.updateState`, `BaseFragment`, `BaseListAdapter`,
`PayloadSet`) и не дублирует его.

### Принципы

| принцип | реализация |
|---|---|
| Единый источник правды: всё состояние экрана, включая тумблеры, приходит из ViewModel | один `StateFlow<S>`; меняется только через `updateState`; флаги загрузки — поля `S`, общий `ListLoadState` — в P8 |
| Однонаправленный поток: UI только отправляет намерения | `dispatch(intent: I)` → `handle(intent)`; намерения — sealed-интерфейс фичи |
| Одноразовые события ViewModel → UI не теряются и не повторяются | `StateViewModel.events`: `Channel(UNLIMITED)` + `receiveAsFlow`; событие ждёт `STARTED` и доставляется один раз; полей `notice` и методов `consume*` нет |
| Мягкое и полное обновление различаются | `RefreshMode.SOFT/HARD` + `Refreshable` |
| Последний запрос побеждает | `LatestLauncher` и делегат `by latest()` / `scope.latestLauncher()` вместо `generation` |
| Невидимые экраны обновляются в фоне по глобальным событиям | `AppEventBus` за интерфейсом, типобезопасные `AppEvent`, `eventsOf<T>()`; `SharedFlowAppEventBus` через Metro `@ContributesBinding`, буфер 64 с `DROP_OLDEST`, публикация не блокирует |
| Списки сравниваются по ключу, перерисовываются только изменившиеся поля | `KeyedDiff(key, payload)` + `aspectPayload(mapOf(Aspect to getter))` → `PayloadSet`, обработка в `bindPayloads` из `core/ui` |
| Обработчики адаптера — один делегат | `IntentAdapterCallback(dispatch, onClick, onLongClick)` превращает клики в намерения; фича подключает его через `by` |
| Общая конфигурация для холдеров | холдер получает типизированный `CB` из `core/ui`; конфигурацию экрана фича добавляет в свой интерфейс `CB`, адаптер реализует его |
| Разовая настройка холдера | `init` или `by lazy`, если настройке нужен первый элемент |
| Зависимости только через Metro, ViewModel без `Context` и UI-эффектов | показы сообщений и переходы — события, их выполняет фрагмент |

### Устройство модуля

- `StateViewModel<S, I, E>`: `state` (из `BaseViewModel`), `events: Flow<E>`,
  `dispatch(I)`, защищённые `handle`, `emit`, `latest()`. Состояние меняется
  только через `updateState`; одноразовые сообщения — только через `emit`.
  Поля `notice` и методы `consume*` больше не нужны.
- `ScreenFragment<VB, S, I, E>`: финальный `onBindingCreated` подписывает
  `render(state)` и `onEvent(event)` на `STARTED`. Фича реализует
  `onScreenCreated`, `render`, `onEvent` и отправляет намерения через
  `dispatch`. ViewModel получается делегатом `by screenViewModel()`.
- `ControlBinder<T>` / `SwitchBinder`: при клике тумблер не меняет своё
  положение. Он отправляет намерение с желаемым значением и сразу возвращается
  к последнему отрисованному state. Новое положение приходит только из
  ViewModel. Программная отрисовка обратный вызов не вызывает.
- `AppEventBus`: публикует событие тот, кто изменил данные (например, редактор).
  ViewModel других экранов подписываются в `viewModelScope` и обновляются в
  фоне (`RefreshMode.SOFT`), пока экран невидим. Шина не заменяет `StateFlow`
  репозиториев: если источник правды уже реактивный, шина не нужна.
- Ограничение: события в `Channel` не переживают смерть процесса. Всё, что
  должно пережить её, хранится в state или в сторе.

### Образцы

- **VPN** (`features/vpn/impl/.../presentation`): `VpnIntent`/`VpnEvent`.
  Решение «отключить / запросить разрешение / подключить» перенесено из
  фрагмента в ViewModel (`VpnEvent.RequestPermission`). Результат системного
  разрешения возвращается намерением `PermissionResult`; установка сертификата
  приходит событием `Open`. Тумблеры «Подключать вне дома» и «Подключать при
  открытии» — `SwitchBinder`. Разметка с обработчиками собрана в `VpnScreen`,
  её проверяют скриншот-тесты и тест кликов
  (`switchesSendIntentsAndShowOnlyTheRenderedState`).
- **История уведомлений** (`features/settings/impl`): пример адаптера —
  `KeyedDiff(NotificationHistoryEntry::id)`, клики через
  `IntentAdapterCallback(::dispatch, HistoryIntent::Open)`; переход и «Очищено»
  приходят событиями `HistoryEvent`.

### Как мигрировать фичу

1. Описать `sealed interface <F>Intent` и `<F>Event` рядом с ViewModel.
2. Унаследовать ViewModel от `StateViewModel<S, I, E>` и разобрать намерения в
   `handle` через `when`. Публичные методы-команды, поля `notice`/`error` для
   одноразовых показов и `consume*` удалить.
3. Перевести фрагмент на `ScreenFragment` и `by screenViewModel()`. Обработчики
   только вызывают `dispatch(...)`. Переключаемые контролы оформить через
   `SwitchBinder`, не читать `isChecked` и не записывать его напрямую.
4. Перезапуски загрузки обернуть в `by latest()` (убирает `refreshJob` и
   `generation`).
5. Адаптер: `KeyedDiff` + `aspectPayload` и `IntentAdapterCallback` через `by`.
6. Если фича меняет данные, которые показывает другой экран без реактивного
   стора, опубликовать `AppEvent`, а на другом экране подписаться через
   `eventsOf<T>()`.
