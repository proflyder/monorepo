# Провайдер входа Windows (Home Komp Logon)

Переиспользуемый Credential Provider для Windows 11: показывает одну плитку
«Home — Автовход» и выполняет действие с настроенной учётной записью
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

Реестр `HKLM\SOFTWARE\Home\KompLogon` (x64):

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

Секрет `C:\ProgramData\Home\KompLogon\secret.bin` — DPAPI в машинной
области (`CRYPTPROTECT_LOCAL_MACHINE`) с фиксированной энтропией
`home-komp-logon-v1`. После расшифровки это JSON
`{"version":1,"password":"…","key":"<base64 32 байта>"}`. Каталог и файл:
System + Administrators полный доступ, больше никого.

## Сборка (macOS/Linux, без Visual Studio)

Провайдер — Rust: `rules_rust` собирает `x86_64-pc-windows-gnu`, линкует
герметичный zig 0.14.1 `cc_toolchain` из `MODULE.bazel` (mingw-w64/UCRT,
статический CRT). Windows SDK, Visual Studio и отдельный mingw-тулчейн не
нужны; Bazel и Bazelisk — корневой build contract репозитория. Bazelisk ставит
`mise install` (скрипты находят его в `PATH`), либо задаётся `BAZELISK`.

```bash
solutions/windows-logon-provider/build.sh         # out/HomeKompLogon.dll, out/cp-selftest.exe
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
`HomeKompLogon.dll`, `cp-selftest.exe` и `SHA256SUMS`. Тег `<VERSION>` неизменяем по содержимому: повторная публикация
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
`C:\Program Files\Home\KompLogon\`, ставит ACL (System/Administrators —
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

`C:\ProgramData\Home\KompLogon\logon.log`, добавление строк, ротация при
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

## Заметки к коду

Пояснения, перенесённые из комментариев исходников (код комментариев не содержит, см. `AGENTS.md`).

- `build.sh` — Cross-build the Windows credential provider with Bazel on macOS or Linux.
- `crates/cp-selftest/src/main.rs` — `cp-selftest.exe`: loads the credential provider outside LogonUI. It changes nothing on the machine, but `SetUsageScenario(CPUS_LOGON)` starts the poller for a moment: run it only when no wake grant is pending.
- `crates/cp-selftest/src/main_windows.rs` — Windows half of `cp-selftest.exe`: tries the registered CLSID, otherwise loads `HomeKompLogon.dll` next to the executable. Output is `PASS`/`FAIL`; exit codes are 0/1.
  - Stop the poller immediately: the self-test must not consume a grant.
- `crates/provider/src/config.rs` — Registry configuration and the DPAPI-protected secret file.
  - `SecureZeroMemory` for memory we do not own as a Rust slice (CoTaskMem buffers, DPAPI output). # Safety `ptr` must be valid for writes of `len` bytes, and `len` must describe the allocation that is wiped (the caller owns it and is about to free it).
  - Absent `Disabled` means 0. A present but unreadable value is still an error: it must not silently change the operator's switch.
  - Reads, DPAPI-decrypts and parses the secret file. `Err` carries a short log-safe phrase (never the secret or the decrypted plaintext).
  - Parses `http://host[:port][/path]` or `https://...`. Rejects userinfo, queries, fragments, whitespace and control characters.
- `crates/provider/src/credential.rs` — The single auto-logon tile.
  - LogonUI deselects tiles during re-enumeration; that must not cancel a fresh arming before the automatic submission.
  - Fail closed: no credential, no status text, no dialog. Re-enumerate so LogonUI drops the tile instead of showing a dead one.
  - LogonUI owns the CoTaskMem copy; the local one must not linger in the Rust heap with the protected password inside.
- `crates/provider/src/dll.rs` — COM in-proc server entry points: exactly `DllGetClassObject` and `DllCanUnloadNow` (`#[unsafe(no_mangle)]`, no `.def`).
- `crates/provider/src/ffi_guard.rs` — `catch_unwind` for every `extern "system"` boundary.
  - `AssertUnwindSafe` is deliberate: a panicking COM call leaves the provider fail-closed (no tile, no armed session) and the shared locks tolerate poisoning, so continuing after the unwind is safe.
  - RAII cleanup step. Its `Drop` catches a panic so a failing cleanup can never unwind out of an `extern "system"` function.
- `crates/provider/src/gate.rs` — Mode decision: logon after a cold boot, unlock of a running locked PC, or off.
  - A WTS call failed: fail closed.
  - `CPUS_UNLOCK_WORKSTATION` without a session of the configured user: never start a claim in that state.
  - The whole decision as one pure function: scenario, session existence, uptime and whether the WTS query succeeded. A `logon` scenario with an existing session of the configured user is the lock screen, not a cold boot, so it selects `unlock` instead.
  - Runs the decision against the live system. Call it before starting the worker, so an off mode never reaches the network.
- `crates/provider/src/http.rs` — Minimal WinHTTP JSON POST client.
  - TLS 1.2+ for https, as decided by the owner; plain `http://` is still accepted for the LAN and pinning is deliberately not done. WinHTTP keeps its normal certificate validation. The option only applies to the session handle, not to the request handle.
  - Belt and braces: a redirect must never carry the mac to another host.
  - `POST path` with a JSON body and an explicit receive timeout (4 s by default, 35 s for the unlock long-poll claim). The send timeout stays at 4 s: the claim body is tiny. Never panics; every failure is a log-safe `Transport` value.
- `crates/provider/src/kerb.rs` — Packing of `KERB_INTERACTIVE_UNLOCK_LOGON`.
- `crates/provider/src/lib.rs` — Windows Credential Provider (in-proc COM server) for Home Komp Logon.
- `crates/provider/src/log.rs` — Append-only diagnostic log for the logon provider.
  - Idempotent: ERROR_ALREADY_EXISTS is expected. The installer owns the ACL; if the directory is missing we simply cannot log.
  - Appends one line. Never panics for I/O reasons and never blocks the caller longer than a single small `WriteFile`.
  - attributes and no template file are passed, and `log_path` is NUL-terminated. The handle is closed on every path. Logging must never break logon: a failed open is ignored.
- `crates/provider/src/provider.rs` — `ICredentialProvider` and the state shared with the credential and the worker.
  - `{9459CCEA-4CCC-4435-89BF-E0E132E0C27F}`. Install/uninstall scripts and tests pin this value; it must never change.
  - Claims the claim loop for `generation`. Returns `false` when that same generation already owns it. A stale, cancelled generation never blocks a newer one: the old worker cannot arm anything because [`arm_session`] checks the generation.
  - LogonUI has not advised us yet: retry shortly, never re-claim.
  - One grant, one attempt: the worker must not arm again.
  - "." means the local SAM: qualify with the computer name so a domain account with the same name can never be picked instead.
  - A verified grant that is waiting for `GetSerialization` needs no second claim; a later `Advise` must not add one.
  - `Ref` is the COM ABI's borrowed-interface wrapper and is `Copy`; taking it by reference would deviate from the vtable signature this helper mirrors and from the `windows-core` conventions.
  - The credential keeps its own Session reference and never calls back into the provider, so releasing it here is safe.
- `crates/provider/src/sync.rs` — Poison-tolerant mutex access.
- `crates/provider/src/wide.rs` — UTF-16 helpers for the Win32 `...W` entry points.
- `crates/provider/src/worker.rs` — The single background poller: `POST /v1/challenge` -> `POST /v1/claim`.
  - Posts on a separate module-pinned thread and returns `None` as soon as the session is cancelled. This is deliberately not implemented by closing the WinHTTP request handle from another thread: Microsoft documents that closing a handle of a *synchronous* …
  - Worker thread procedure. Owns `parameter` and never returns. The whole body — the loop and its cleanup — is contained. A panic cancels the session (fail closed), `finish_worker` and the worker count are released exactly once, and the module pin is always …
  - Two independent steps: a panic in the first must not skip the worker-count decrement.
  - The thread never started, so it did not take the argument.
  - An error back-off is never shortened by the minimum.
- `crates/provider/src/worker_core.rs` — Portable challenge/claim exchange.
  - LogonUI has not advised the provider yet; retry shortly, never re-claim.
  - The grant is already consumed, so never claim again: only wait a moment for Advise() and then give up.
  - The grant is never claimed twice.
- `fetch.sh` — Fetch and verify a published provider artifact on the operator's Mac/Linux.
- `install.ps1` — <# .SYNOPSIS Устанавливает провайдер входа Home для ПК «комп».
  - --- LogonUI registration (credential provider, never a filter) --------------
- `pe_artifacts.py` — PE checks for the Windows logon provider artifacts.
- `publish-oci.sh` — Publish the built provider as a public OCI artifact in GHCR with ORAS.
  - Verify the checksums before publishing, so a broken artifact never reaches the registry.
  - Only a definitively missing tag means "safe to publish". Any other failure (auth, network, registry error) must not silently fall through to a push that could overwrite an existing version tag.
- `regen-bindings.sh` — Regenerate crates/provider/src/bindings.rs and crates/cp-selftest/src/bindings.rs from the filters in each crate's bindings.txt. Generation is manual and not part of the build.
- `run-tests.sh` — Host tests, rustfmt and clippy for host and Windows targets.
- `set-secret.ps1` — <# .SYNOPSIS Записывает DPAPI-защищённый секрет провайдера входа Home.
- `status.ps1` — <# .SYNOPSIS Показывает состояние провайдера входа Home (без секретов).
- `test_install_scripts.py` — Static contract tests for the Windows logon provider scripts.
- `test_oci_scripts.py` — Static contract tests for the OCI distribution scripts.
  - The home NodePort and machine name must not be defaults here.
- `uninstall.ps1` — <# .SYNOPSIS Удаляет провайдер входа Home с ПК «комп».
