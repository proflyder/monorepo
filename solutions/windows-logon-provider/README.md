# Провайдер входа Windows (Ricardo Komp Logon)

Переиспользуемый Credential Provider для Windows 11: показывает одну плитку
«Ricardo — Автовход» и входит в настроенную учётную запись **только** тогда,
когда сервис `wakegrant` выдал одноразовый грант (контракт
`packages/contracts/wakegrant/PROTOCOL.md`).

- выключенный по кнопке ПК → гранта нет → плитки нет, обычный вход по PIN
  работает как раньше;
- любая ошибка (нет конфигурации, секрета, сети, неверный MAC, включённый
  `Disabled`) → плитки нет, диалогов нет, зависаний нет;
- грант расходуется на одну попытку: после сериализации плитка снимается,
  фоновый поток останавливается.

Service URL, имя машины и учётная запись задаёт проект при установке; в самом
решении этих значений нет. Домашняя установка для ПК «комп» описана в
`projects/home/windows-pc/README.md`.

## Как это работает

1. LogonUI создаёт провайдера и вызывает `SetUsageScenario`. Гранты
   запрашиваются **только** в `CPUS_LOGON` (холодная загрузка → вход).
   `CPUS_UNLOCK_WORKSTATION` принимается, но ничего не делает: ни плитки, ни
   фонового потока, ни сети, `GetCredentialCount` = 0 — грант никогда не
   должен разблокировать уже работающий ПК. Остальные сценарии → `E_NOTIMPL`.
2. Провайдер запускает **один** фоновый поток (`CreateThread`, не UI-поток).
   Бюджет — 90 секунд от загрузки провайдера, интервал опроса 2 секунды,
   WinHTTP-таймауты 2/2/4/4 с, `WINHTTP_ACCESS_TYPE_NO_PROXY`, без редиректов
   и cookie.
3. Поток читает конфигурацию и секрет, затем циклически:
   `POST /v1/challenge` → `POST /v1/claim`.
4. Ответ `/v1/claim` проверяется по HMAC (свой nonce, `grants`/`granted`);
   только `granted:true` с валидным MAC «взводит» плитку и один раз вызывает
   `CredentialsChanged`.
5. `GetCredentialCount` → `1`, default `0`, `autoLogonWithDefault = TRUE`;
   LogonUI сразу вызывает `GetSerialization`.
6. `GetSerialization` расшифровывает DPAPI-блоб в этот момент, собирает
   `KERB_INTERACTIVE_UNLOCK_LOGON` (`.\<user>`), при необходимости
   `CredProtect`, затирает пароль `SecureZeroMemory` и возвращает
   `CPGSR_RETURN_CREDENTIAL_FINISHED`.
7. `ReportResult` с ошибкой входа увеличивает `FailCount`; на третьем —
   `Disabled=1`, после чего провайдер не делает ничего (ни сети, ни плитки)
   до переустановки.

## Файлы

| файл | назначение |
|---|---|
| `crates/protocol/` | переносимый слой: base64, HMAC-сообщения, разбор JSON, проверка MAC, конфиг |
| `crates/provider/src/provider.rs` | `ICredentialProvider`, общее состояние `Session`, бюджет |
| `crates/provider/src/credential.rs` | плитка, `GetSerialization`, `ReportResult`, `GetUserSid` |
| `crates/provider/src/worker.rs` | единственный фоновый поток challenge/claim |
| `crates/provider/src/config.rs` | реестр, DPAPI, `FailCount`/`Disabled` |
| `crates/provider/src/http.rs`, `log.rs` | WinHTTP, журнал; HMAC — из crates.io |
| `crates/provider/src/kerb.rs` | упаковка `KERB_INTERACTIVE_UNLOCK_LOGON` (MIT, см. `THIRD-PARTY-NOTICES.md`) |
| `crates/provider/src/dll.rs` | `DllGetClassObject`, `DllCanUnloadNow` |
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
| `Disabled` | DWORD | `1` — провайдер неактивен |
| `FailCount` | DWORD | счётчик неудачных входов, ставит сам провайдер |

Секрет `C:\ProgramData\Ricardo\KompLogon\secret.bin` — DPAPI в машинной
области (`CRYPTPROTECT_LOCAL_MACHINE`) с фиксированной энтропией
`ricardo-komp-logon-v1`. После расшифровки это JSON
`{"version":1,"password":"…","key":"<base64 32 байта>"}`. Каталог и файл:
System + Administrators полный доступ, больше никого.

## Сборка (macOS/Linux, без Visual Studio)

Провайдер — Rust: `rules_rust` собирает `x86_64-pc-windows-gnu`, линкует
герметичный zig 0.14.1 `cc_toolchain` из `MODULE.bazel` (mingw-w64/UCRT,
статический CRT). Windows SDK, Visual Studio и отдельный mingw-тулчейн не
нужны; Bazel и Bazelisk — корневой build contract репозитория. Локально Bazelisk лежит в
`secrets/local-tools/bazelisk` (скрипты находят его сами), либо `bazel` на
`PATH`/`BAZELISK`.

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
combase, ole32, oleaut32, rpcrt4, crypt32, secur32, shlwapi, userenv,
winhttp, ws2_32, bcryptprimitives и api-ms-win-*); всё остальное — ошибка.
Проверка не требует Windows-тулчейна на хосте.

`run-tests.sh` запускает `bazel test //solutions/windows-logon-provider/...`:
переносимые protocol-векторы и безопасное ядро провайдера на host, плюс
rustfmt/clippy для host- и Windows-целей (aspect переключается на Windows
платформу, поэтому проверяется настоящий `#[cfg(windows)]`-код).

`out/cp-selftest.exe` загружает провайдера по CLSID (или из DLL рядом с exe,
если он ещё не зарегистрирован) и печатает число полей и плиток. Он запускает
и сразу останавливает фоновый поток, поэтому проверять им стоит только когда
грант не ожидается.

## Публикация и установка из GHCR

Версия решения — `VERSION` (semver). На push в `develop`, когда затронут юнит
`logon-provider`, `deploy-home` собирает его тем же `_native.yml`, что и PR,
затем `publish-oci.sh` публикует публичный OCI-артефакт:

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
# нужны oras и авторизованный gh; oras можно положить в secrets/local-tools/
solutions/windows-logon-provider/fetch.sh 1.1.0
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
2026-11-03T21:44:12.130Z pid=912 tid=1204 worker phase=start budget_ms=90000
2026-11-03T21:44:12.400Z pid=912 tid=1204 grant result=granted grant_id=… ms=41 elapsed_ms=270
2026-11-03T21:44:12.401Z pid=912 tid=1204 armed ok=1
2026-11-03T21:44:12.520Z pid=912 tid=913 serialization elapsed_ms=3
2026-11-03T21:44:13.900Z pid=912 tid=913 report_result status=success
```

События: `provider_load/unload`, `scenario`, `worker`, `config`, `secret`,
`http`, `challenge`, `claim`, `grant`, `armed`, `cancel`, `serialization`,
`serialization_config`, `serialization_secret`, `report_result`. Пароль, ключ
и MAC в журнал не попадают; все значения очищаются от управляющих символов.

## Ограничения

- Проверено только сборкой и переносимыми тестами; реальный экран входа,
  замок и переключатель «только Windows Hello» требуют проверки на самом ПК.
- Опрос каждые 2 с даёт до ~45 challenge за 90 с при лимите 30/мин на машину:
  на длинном окне сервис может ответить `429`, тогда провайдер отступает на
  4 с и продолжает; на реальный сценарий (грант выдан до Wake-on-LAN) это не
  влияет — первый claim успевает раньше.
- `SetUsageScenario` не блокирует вход: `CPUS_UNLOCK_WORKSTATION` молча
  ничего не делает (без сети и плитки); грант выдаётся только для `CPUS_LOGON`.
- `Disabled` снимается только переустановкой (`install.ps1` сбрасывает
  `FailCount` и `Disabled`) или правкой реестра.
- Выключение провайдера не влияет на обычный вход по PIN.

## Лицензия

Собственный код — под лицензией репозитория. Файлы, производные от
Microsoft Windows-classic-samples (MIT), сохраняют уведомление об авторских
правах; полный текст — `THIRD-PARTY-NOTICES.md`.
