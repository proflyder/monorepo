# Провайдер входа Windows (Ricardo Komp Logon)

Переиспользуемый Credential Provider для Windows 11: показывает одну плитку
«Ricardo — Автовход» и выполняет действие с настроенной учётной записью
**только** тогда, когда сервис `wakegrant` выдал одноразовый грант нужного
назначения (контракт `packages/contracts/wakegrant/PROTOCOL.md`):

- `logon` — вход после холодной загрузки (Wake-on-LAN);
- `unlock` — разблокировка уже работающего, заблокированного ПК.

Гранты двух назначений хранятся, запрашиваются и проверяются по MAC
раздельно: грант `logon` никогда не разблокирует сессию, грант `unlock`
никогда не используется для входа.

- выключенный по кнопке ПК → гранта нет → плитки нет, обычный вход по PIN
  работает как раньше;
- любая ошибка (нет конфигурации, секрета, сети, неверный MAC, включённый
  `Disabled`) → плитки нет, диалогов нет, зависаний нет;
- грант расходуется на одну попытку: после сериализации плитка снимается,
  фоновый поток останавливается (в режиме `unlock` — до остановки
  провайдера или следующей блокировки).

Service URL, имя машины и учётная запись задаёт проект при установке; в самом
решении этих значений нет. Домашняя установка для ПК «комп» описана в
`projects/home/windows-pc/README.md`.

## Как это работает

1. LogonUI создаёт провайдера и вызывает `SetUsageScenario`.
   Поддерживаются `CPUS_LOGON` и `CPUS_UNLOCK_WORKSTATION`; остальные —
   `E_NOTIMPL`. Windows 11 вызывает `CPUS_LOGON` и после `LockWorkStation`,
   поэтому сценария недостаточно: до запуска потока чистая функция
   `gate::decide_mode(сценарий, есть_сессия, аптайм, запрос_WTS_успешен)`
   выбирает режим.
   - **Сессия пользователя.** `WTSEnumerateSessionsW` +
     `WTSQuerySessionInformationW` ищут любую сессию настроенной учётной
     записи в любом состоянии (Active, Disconnected и т.д.); сессия 0 и
     пустое имя игнорируются. Сравнивается только имя пользователя без учёта
     регистра: домен из WTS (NetBIOS, полное имя, учётка Microsoft) может не
     совпасть с настроенным, и лишний отказ здесь безопаснее лишнего входа.
     Нашлась — режим `unlock` независимо от аптайма. Нет сессии и аптайм
     меньше 10 минут — режим `logon`.
   - **Отказ.** `CPUS_UNLOCK_WORKSTATION` без сессии настроенного
     пользователя, ошибка WTS-вызова, нечитаемая конфигурация или
     `Disabled` → `off`: ни плитки, ни потока, ни сети,
     `GetCredentialCount` = 0. В журнале
     `scenario cpus=logon mode=off reason=...`.
2. Провайдер запускает **один** фоновый поток (`CreateThread`, не UI-поток).
   Он читает конфигурацию и секрет, затем циклически выполняет
   `POST /v1/challenge` → `POST /v1/claim`.
   - **logon:** `"purpose":"logon"`, опрос каждые 2 секунды без
     `waitSeconds`, бюджет 90 секунд от загрузки провайдера.
   - **unlock:** `"purpose":"unlock"` и `"waitSeconds":25` (long-poll), без
     бюджета — цикл живёт, пока провайдер не остановлен. Отказ сервера
     (`denied`) сразу начинает следующий раунд: паузу даёт сам long-poll.
     Раунд короче 2 секунд (сервер не держит long-poll) добирается до этого
     минимума, чтобы не крутить поток на каждый мгновенный `denied`.
     Ошибка сети, `429` или `5xx` — пауза 5/10/20/30 секунд (backoff,
     насыщается на 30). Остальные HTTP-статусы — остановка (fail closed).
   WinHTTP-таймауты 2/2/4/4 с; у unlock-claim receive timeout 35 секунд
   (больше `waitSeconds`). `WINHTTP_ACCESS_TYPE_NO_PROXY`, без редиректов и
   cookie. Для `https://` минимальная версия TLS — 1.2 (WinHTTP
   `WINHTTP_OPTION_SECURE_PROTOCOLS`, TLS 1.2 | 1.3), сертификат проверяется
   системно; pinning не делается, plain `http://` в LAN разрешён.
3. Ответ `/v1/claim` проверяется по HMAC со своим nonce, `granted` и
   **purpose**; только `granted:true` с валидным MAC нужного назначения
   «взводит» плитку и один раз вызывает `CredentialsChanged`. MAC для
   другого purpose отвергается.
4. Остановка. `UnAdvise`, смена сценария и сериализация увеличивают
   `cancel_generation`; поток проверяет его до и после каждого запроса.
   Unlock-claim выполняется на отдельном короткоживущем потоке с
   закреплённым модулем: воркер перестаёт ждать long-poll в течение ~100 мс,
   а сам запрос ограничен receive timeout. Закрывать WinHTTP-хендл
   синхронного запроса из другого потока нельзя (документированная гонка),
   поэтому запрос просто доигрывается и ничего не может взвести.
5. `GetCredentialCount` → `1`, default `0`, `autoLogonWithDefault = TRUE`;
   LogonUI сразу вызывает `GetSerialization`.
6. `GetSerialization` расшифровывает DPAPI-блоб в этот момент, собирает
   `KERB_INTERACTIVE_UNLOCK_LOGON` (`.\<user>`; для
   `CPUS_UNLOCK_WORKSTATION` — `KERB_WORKSTATION_UNLOCK_LOGON`), при
   необходимости `CredProtect`, затирает пароль `SecureZeroMemory` и
   возвращает `CPGSR_RETURN_CREDENTIAL_FINISHED`.
7. `ReportResult` с ошибкой увеличивает `FailCount`; на третьем —
   `Disabled=1`, после чего провайдер не делает ничего (ни сети, ни плитки)
   до переустановки. В обоих режимах.

## Файлы

| файл | назначение |
|---|---|
| `packages/rust/wakegrant-protocol/` | переносимый слой (общий с сервисом): `serde`/`serde_json`, `base64`, HMAC-сообщения, проверка MAC; конфиг провайдера — `crates/provider/src/config.rs` |
| `crates/provider/src/provider.rs` | `ICredentialProvider`, общее состояние `Session`, бюджет |
| `crates/provider/src/gate.rs` | режим `logon`/`unlock`/`off` по WTS-сессии, аптайму и сценарию |
| `crates/provider/src/credential.rs` | плитка, `GetSerialization`, `ReportResult`, `GetUserSid` |
| `crates/provider/src/worker.rs` | единственный фоновый поток challenge/claim, long-poll и прерывание |
| `crates/provider/src/config.rs` | реестр, DPAPI, `FailCount`/`Disabled` |
| `crates/provider/src/http.rs`, `log.rs` | WinHTTP, журнал; HMAC — из crates.io |
| `crates/provider/src/kerb.rs` | упаковка `KERB_INTERACTIVE_UNLOCK_LOGON` (MIT, см. `THIRD-PARTY-NOTICES.md`) |
| `crates/provider/src/dll.rs` | `DllGetClassObject`, `DllCanUnloadNow` |
| `crates/provider/src/bindings.rs`, `crates/*/bindings.txt` | сгенерированные Win32-привязки и их фильтры |
| `regen-bindings.sh` | перегенерация `bindings.rs` (вручную, не в сборке) |
| `crates/cp-selftest/` | `cp-selftest.exe`: загрузка провайдера вне LogonUI |
| `install.ps1`, `set-secret.ps1`, `uninstall.ps1`, `status.ps1` | установка и обслуживание |
| `build.sh`, `run-tests.sh`, `pe_artifacts.py` | сборка Bazel, host-тесты и проверка PE |
| `VERSION`, `publish-oci.sh`, `fetch.sh` | версия и публикация/получение OCI-артефакта |

CLSID провайдера (зафиксирован, менять нельзя без правки всех скриптов и
теста): `{9459CCEA-4CCC-4435-89BF-E0E132E0C27F}`.

## Конфигурация

Реестр `HKLM\SOFTWARE\Ricardo\KompLogon` (x64):

| значение | тип | смысл |
|---|---|---|
| `ServiceUrl` | REG_SZ | `http://host:port[/path]` сервиса wakegrant |
| `Machine` | REG_SZ | `<machine>` (`^[a-z0-9-]{1,32}$`) |
| `UserName` | REG_SZ | локальная учётная запись, например `<user>` |
| `Domain` | REG_SZ | `.` для локальной учётной записи |
| `Disabled` | DWORD | `1` — провайдер неактивен; отсутствует = `0` |
| `FailCount` | DWORD | счётчик неудачных входов, ставит сам провайдер; отсутствует = `0` |

`Disabled` проверяется раньше остальных значений: при `1` провайдер
выключается, даже если `ServiceUrl` или имена в реестре испорчены. Отсутствие
`Disabled`/`FailCount` провайдер не выключает.

Секрет `C:\ProgramData\Ricardo\KompLogon\secret.bin` — DPAPI в машинной
области (`CRYPTPROTECT_LOCAL_MACHINE`) с фиксированной энтропией
`ricardo-komp-logon-v1`. После расшифровки это JSON
`{"version":1,"password":"…","key":"<base64 32 байта>"}`. Каталог и файл:
System + Administrators полный доступ, больше никого.

## Сборка (macOS/Linux, без Visual Studio)

Провайдер — Rust: `rules_rust` собирает `x86_64-pc-windows-gnu`, линкует
герметичный zig 0.14.1 `cc_toolchain` из `MODULE.bazel` (mingw-w64/UCRT,
статический CRT). Windows SDK, Visual Studio и отдельный mingw-тулчейн не
нужны; Bazel и Bazelisk — корневой build contract репозитория. Bazelisk ставит
`mise install` (скрипты находят его в `PATH`), либо задаётся `BAZELISK`.

```bash
solutions/windows-logon-provider/build.sh         # out/RicardoKompLogon.dll, out/cp-selftest.exe
solutions/windows-logon-provider/run-tests.sh     # host-тесты, rustfmt/clippy (host и Windows)
```

`build.sh` собирает цель
`//solutions/windows-logon-provider/crates/provider:provider` под платформой
`//platform/bazel/windows:x86_64_windows`, копирует артефакты и делает PE
воспроизводимым (обнуляет TimeDateStamp). Затем `pe_artifacts.py` проверяет,
что DLL экспортирует ровно `DllGetClassObject` и `DllCanUnloadNow` и
импортирует только системные библиотеки (KERNEL32, ntdll, advapi32,
combase, oleaut32, rpcrt4, crypt32, secur32, shlwapi, userenv,
winhttp, ws2_32, wtsapi32, bcryptprimitives и api-ms-win-*); всё остальное —
ошибка.
Проверка не требует Windows-тулчейна на хосте.

`run-tests.sh` запускает `bazel test //solutions/windows-logon-provider/...`:
переносимые protocol-векторы и безопасное ядро провайдера на host, плюс
rustfmt/clippy для host- и Windows-целей (aspect переключается на Windows
платформу, поэтому проверяется настоящий `#[cfg(windows)]`-код).

`out/cp-selftest.exe` загружает провайдера по CLSID (или из DLL рядом с exe,
если он ещё не зарегистрирован) и печатает число полей и плиток. Он запускает
и сразу останавливает фоновый поток, поэтому проверять им стоит только когда
грант не ожидается.

### Win32-привязки

Переносимый слой использует стандартный стек (решение 40): `serde` и
`serde_json` с `deny_unknown_fields`, `base64`, `thiserror`, `http::Uri`
и `home-secret` (zeroizing-пароль и redacted `Debug`). Собственных
JSON/base64/ошибочных типов в решении нет.

`crates/*/src/bindings.rs` сгенерированы `windows-bindgen 0.63.0` из фильтров
`crates/*/bindings.txt` (`--no-comment --implement`); поэтому для Win32
достаточно `windows-core` и `windows-link`, без крейта `windows`. Генерация не
входит в сборку. После изменения фильтров:

```bash
solutions/windows-logon-provider/regen-bindings.sh
```

Скрипт собирает pinned bindgen во временном cargo-проекте, перезаписывает оба
`bindings.rs` и прогоняет `rustfmt`; файлы коммитятся вместе с фильтрами.

## Публикация и установка из GHCR

Версия решения — `VERSION` (semver). Любое изменение путей провайдера,
общих `Cargo.lock`/`Cargo.toml`/`crate-universe.lock`, используемых крейтов
`packages/rust/*` или тулчейнов Windows/zig попадает в PR CI: юнит
`logon-provider` собирает DLL и `cp-selftest`, гоняет `run-tests.sh`,
проверки `pe_artifacts.py` и `SHA256SUMS`, и из этого репозитория публикует их
под `tree-<hash>` с provenance (build once). На push в `develop`
`native-release` продвигает этот digest на теги `VERSION`/`sha-<12>`, ничего не
собирая: если `tree-<hash>` не опубликован или provenance не сходится, job
падает с подсказкой перезапустить PR CI. `publish_when` по
`solutions/windows-logon-provider/VERSION` ограничивает релиз. Публичный
OCI-артефакт:

```text
ghcr.io/proflyder/windows-logon-provider:<VERSION>
ghcr.io/proflyder/windows-logon-provider:sha-<12 hex коммита>
```

Тип артефакта — `application/vnd.<owner>.windows-logon-provider.v1`; файлы:
`RicardoKompLogon.dll`, `cp-selftest.exe` и `SHA256SUMS`. Тег `<VERSION>` неизменяем по содержимому: повторная публикация
с тем же `SHA256SUMS` — успешный no-op, с другим — ошибка. Поэтому релиз
делается сменой `VERSION`. Provenance attestation создаётся для digest
опубликованного артефакта (`push-to-registry`) и для файла DLL.

Оператор на Mac/Linux получает артефакт по версии или digest:

```bash
# нужны oras и авторизованный gh (оба — из `mise install`)
solutions/windows-logon-provider/fetch.sh <VERSION>
solutions/windows-logon-provider/fetch.sh sha256:<digest>   # рекомендуемый pin
```

`fetch.sh` делает `oras pull` во временный каталог, проверяет `SHA256SUMS`
(`sha256sum -c` / `shasum -a 256 -c`) и provenance через
`gh attestation verify ... --owner <owner>` (owner берётся из имени пакета).
Любое расхождение, отсутствие инструмента или неудачная аттестация — ненулевой
код. В конце печатается разрешённый digest; каталог с файлами сохраняется.

Пакет GHCR создаётся первым publish. Если он оказался приватным (типично для
нового пакета), один раз в GitHub UI: **профиль владельца → Packages →
`windows-logon-provider` → Package settings → Danger Zone → Change
visibility → Public**. После этого анонимный `oras pull` работает без
токена.

Установка (PowerShell от администратора; `out` — распакованный артефакт):

```powershell
cd solutions\windows-logon-provider
powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 `
    -ServiceUrl 'http://192.0.2.10:8080' `
    -Machine '<machine>' -UserName '<user>'

# пароль спрашивается интерактивно, ключ — одной строкой в stdin
'BASE64KEY' | powershell -NoProfile -ExecutionPolicy Bypass -File set-secret.ps1

powershell -NoProfile -ExecutionPolicy Bypass -File status.ps1
```

`install.ps1` требует `-ServiceUrl`, `-Machine` и `-UserName`: значения
приходят от проекта при установке, а не из решения. Скрипт копирует DLL в
`C:\Program Files\Ricardo\KompLogon\`, ставит ACL (System/Administrators —
запись, Users — чтение/запуск), пишет конфигурацию, регистрирует
`HKLM\SOFTWARE\Classes\CLSID\{…}\InprocServer32`
(`ThreadingModel = Apartment`, это и есть машинная часть `HKCR`) и ключ в
`HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Authentication\Credential Providers\{…}`.
Других провайдеров и фильтры скрипт не трогает и не регистрирует.

Удаление (регистрации снимаются сразу, без перезагрузки; логи остаются):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File uninstall.ps1
```

## Журнал

`C:\ProgramData\Ricardo\KompLogon\logon.log`, добавление строк, ротация при
256 КиБ в `logon.log.1` (одно поколение). Формат:

```
2026-11-03T21:44:12.123Z pid=912 tid=913 provider_load
2026-11-03T21:44:12.128Z pid=912 tid=913 scenario cpus=logon mode=logon
2026-11-03T21:44:12.130Z pid=912 tid=1204 worker phase=start mode=logon budget_ms=90000
2026-11-03T21:44:12.400Z pid=912 tid=1204 grant result=granted purpose=logon grant_id=… ms=41 elapsed_ms=270
2026-11-03T21:44:12.401Z pid=912 tid=1204 armed ok=1
2026-11-03T21:44:12.520Z pid=912 tid=913 serialization elapsed_ms=3
2026-11-03T21:44:13.900Z pid=912 tid=913 report_result status=success
2026-11-03T22:10:00.100Z pid=912 tid=913 scenario cpus=logon mode=unlock
2026-11-03T22:10:00.102Z pid=912 tid=1209 worker phase=start mode=unlock wait_s=25
2026-11-03T22:10:31.500Z pid=912 tid=1209 grant result=granted purpose=unlock grant_id=… ms=25031 elapsed_ms=31320
2026-11-03T22:10:31.501Z pid=912 tid=1209 armed ok=1
2026-11-03T22:10:31.600Z pid=912 tid=913 worker phase=armed
2026-11-03T22:12:05.000Z pid=912 tid=913 cancel reason=unadvise
2026-11-03T22:12:05.050Z pid=912 tid=1211 worker phase=cancelled
```

События: `provider_load/unload`, `scenario`, `worker`, `config`, `secret`,
`http`, `challenge`, `claim`, `grant`, `armed`, `cancel`, `serialization`,
`serialization_config`, `serialization_secret`, `report_result`. События
идут через `tracing`, но строку рендерит провайдерский слой в Win32-писатель:
формат, ротация и отсутствие stderr — часть контракта LogonUI. В режиме
`unlock` отказы (`denied`) не пишутся — за ночь их были бы тысячи; ошибки
пишутся с подавлением повторов, остановка и `grant result=granted
purpose=unlock` — всегда. Пароль, ключ и MAC в журнал не попадают; все
значения очищаются от управляющих символов.

## Ограничения

- Режим `logon` опрашивает каждые 2 с: до ~45 challenge за 90 с при лимите
  30/мин на машину, поэтому сервис может ответить `429`; провайдер отступает
  на 4 с и продолжает. На реальный сценарий (грант выдан до Wake-on-LAN) это
  не влияет — первый claim успевает раньше.
- Режим `unlock` держит long-poll по 25 с (около 2–3 challenge в минуту), пока
  провайдер жив. Если провайдера остановили, пока запрос в полёте, поток
  запроса доигрывает не дольше receive timeout (35 с), ничего не взводит и не
  держит DLL: модуль закреплён до его завершения.
- `SetUsageScenario` не блокирует вход: `CPUS_LOGON` без сессии настроенного
  пользователя требует аптайма меньше 10 минут; `CPUS_UNLOCK_WORKSTATION` без
  такой сессии ничего не делает (без сети и плитки).
- `Disabled` снимается только переустановкой (`install.ps1` сбрасывает
  `FailCount` и `Disabled`) или правкой реестра.
- Выключение провайдера не влияет на обычный вход по PIN.

## Лицензия

Собственный код — под лицензией репозитория. Файлы, производные от
Microsoft Windows-classic-samples (MIT), сохраняют уведомление об авторских
правах; полный текст — `THIRD-PARTY-NOTICES.md`.
