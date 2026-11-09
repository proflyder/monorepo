# Комп — питание рабочего Windows-ПК

Голосовое и приложенческое управление проводным Windows-десктопом «комп»
(алиас «мой комп»):

- включение — Wake-on-LAN (magic packet);
- выключение — `shutdown /s /t 60` через ограниченный SSH, с отменой;
- блокировка — запуск задачи планировщика `\Home\KompLock` через тот же SSH;
- разблокировка — одноразовый грант `wakegrant` (модуль входа, см. ниже);
- состояние — ICMP ping из Home Assistant и, для запущенного ПК, признак
  блокировки экрана по SSH-слову `status`.

HA-часть — `projects/home-assistant/ha-voice/` (см. её README). Реальные
IP/MAC/имя пользователя живут только в HA `secrets.yaml`; в Git их нет, здесь
используется TEST-NET `192.0.2.0/24` и плейсхолдеры.

## Модуль входа (logon provider)

Провайдер входа — переиспользуемое решение
`solutions/windows-logon-provider` (сборка, публикация в GHCR, self-test и
инструкции по установке — в его README). В нём нет домашних значений: при
установке на этот ПК передаются параметры проекта (адрес — TEST-NET
плейсхолдер, заменить на реальный):

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 `
    -ServiceUrl 'http://192.0.2.27:30130' `
    -Machine 'komp' -UserName '<user>'
```

`switch.komp.turn_on` выдаёт грант `wakegrant` до Wake-on-LAN; без гранта
плитки нет и работает обычный вход по PIN. Порядок получения артефакта,
проверки digest и замены DLL — `docs/OPERATIONS.md`, раздел «Модуль входа
(logon provider)».

## Чеклист Windows

1. **BIOS/UEFI.** Включить Wake-on-LAN / «Power On by PCI-E» (название зависит
   от платы), выключить «ErP/EuP» если он глушит дежурное питание NIC.
2. **Сетевой адаптер.** Свойства → Управление электропитанием: разрешить
   устройству будить компьютер, разрешить «только magic packet». В
   Дополнительно: `Wake on Magic Packet` = Enabled.
3. **Fast Startup / быстрый запуск.** Выключить: Панель управления →
   Электропитание → «Действия кнопок питания» → снять «Включить быстрый
   запуск». Иначе S5 может быть гибридным и WoL ведёт себя непредсказуемо.
4. **ICMP.** Брандмауэр Windows → правило «File and Printer Sharing (Echo
   Request – ICMPv4-In)» включить для частного профиля. HA должен получать
   ответ на `ping` без авторизации.
5. **Адрес.** DHCP-резервация или статический IP по проводу; MAC — тот, что
   виден в `ipconfig /all` у Ethernet-адаптера.

## OpenSSH Server

```powershell
Add-WindowsCapability -Online -Name OpenSSH.Server~~~~0.0.1.0
Set-Service -Name sshd -StartupType Automatic
Start-Service sshd
```

Порт 22 по умолчанию, входящее правило Windows создаёт само. Для admin-пользователя
ключ кладётся не в `%USERPROFILE%\.ssh`, а в общий файл:

```powershell
New-Item -ItemType Directory -Force C:\ProgramData\Home
# скопировать komp-power.ps1 в C:\ProgramData\Home\komp-power.ps1
New-Item -ItemType File -Force C:\ProgramData\ssh\administrators_authorized_keys
icacls.exe "C:\ProgramData\ssh\administrators_authorized_keys" /inheritance:r `
  /grant "Administrators:F" /grant "SYSTEM:F"
```

Строка в `administrators_authorized_keys` (одна строка, без переводов внутри):

```text
from="192.0.2.28",restrict,command="powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\ProgramData\Home\komp-power.ps1" ssh-ed25519 AAAA... ha-komp
```

- `from="192.0.2.28"` — адрес HA (TEST-NET-плейсхолдер, заменить на реальный);
  ключ не сработает с другого адреса.- `command=` — forced command: клиент не может запустить ничего, кроме
  `komp-power.ps1`, а тот принимает только `shutdown`, `cancel`, `lock` и
  `status`.
- `restrict` отключает port/agent/X11 forwarding и pty.
- `AAAA...` — публичный ключ (`/config/.ssh/komp_ed25519.pub` в HA).

Не-admin пользователь использует обычный `%USERPROFILE%\.ssh\authorized_keys`
(файл и каталог должны принадлежать только ему); для admin-пользователя
Win32-OpenSSH читает `administrators_authorized_keys` — поэтому `icacls`
выше обязателен, иначе `sshd` игнорирует файл.

`komp-power.ps1` хранить в UTF-8 **с BOM**: Windows PowerShell 5.1 иначе
читает файл в ANSI и портит русский текст в диалоге выключения.

## Блокировка экрана

SSH-сессия HA идёт в служебной сессии OpenSSH, поэтому прямой
`LockWorkStation` оттуда не блокирует экран пользователя. Вместо этого
`komp-power.ps1 lock` запускает задачу планировщика `\Home\KompLock`, а задача
выполняет `rundll32.exe user32.dll,LockWorkStation` в интерактивной сессии.

Задачу создают **один раз** из PowerShell от администратора в сессии того же
пользователя, который работает за ПК:

```powershell
# скопировать install-lock-task.ps1 на ПК, затем:
powershell -NoProfile -ExecutionPolicy Bypass -File C:\ProgramData\Home\install-lock-task.ps1
```

Скрипт регистрирует `\Home\KompLock` с principal `Interactive`
(«только когда пользователь вошёл», без пароля) и без триггера: задача
запускается только по `schtasks /run`. Повторный запуск безопасно заменяет
задачу. Проверка:

```powershell
schtasks /query /tn '\Home\KompLock' /v /fo LIST
schtasks /run /tn '\Home\KompLock'   # экран должен заблокироваться
```

## Состояние блокировки

`komp-power.ps1 status` — только чтение процессов, без запуска чего-либо:

- `locked`, если `LogonUI.exe` работает в интерактивной сессии пользователя;
- `unlocked`, если его там нет;
- если интерактивной оболочки (`explorer.exe`) нет вообще — это экран входа,
  он тоже считается `locked`.

Сессию пользователя определяет `explorer.exe` (интерактивная оболочка), поэтому
служебная SSH-сессия OpenSSH (session 0) не может дать ложный `locked`.
`LogonUI.exe` появляется на экране блокировки, экране входа и на короткое
время на UAC-запросе; опрос раз в 30 с такой короткий всплеск обычно не
замечает. Команда печатает ровно `locked` или `unlocked` и всегда завершается
кодом 0; проверка:

```bash
ssh -i /config/.ssh/komp_ed25519 \
  -o BatchMode=yes -o ConnectTimeout=5 \
  -o UserKnownHostsFile=/config/.ssh/komp_known_hosts \
  -o StrictHostKeyChecking=accept-new <user>@<host> status
```

## Ключ и known_hosts в HA

В HA: Terminal & SSH add-on.

```bash
mkdir -p /config/.ssh && chmod 700 /config/.ssh
ssh-keygen -t ed25519 -f /config/.ssh/komp_ed25519 -N '' -C ha-komp
ssh-keyscan -H '<host>' >> /config/.ssh/komp_known_hosts
```

`komp_ed25519` (0600) и `komp_known_hosts` остаются в `/config/.ssh` — это
персистентный каталог контейнера HA. Публичный ключ добавить на Windows (выше).

## HA secrets

Только в `secrets.yaml` (не в Git):

```yaml
komp_mac: "AA:BB:CC:DD:EE:FF"
komp_ssh_target: "<user>@<host>"
komp_ping_command: "ping -c 1 -W 1 <host> >/dev/null 2>&1 && echo 1 || echo 0"
```

`komp_ping_command` — целиком, потому что HA не умеет склеивать `!secret` с
текстом; `komp_ssh_target` подставляется в `shell_command` через
`input_text.komp_ssh_target`.

## Проверка

Из Terminal & SSH add-on (HA):

```bash
# ping
ping -c 1 -W 1 <host>

# SSH: должно запланировать выключение через 60 секунд
ssh -i /config/.ssh/komp_ed25519 \
  -o BatchMode=yes -o ConnectTimeout=5 \
  -o UserKnownHostsFile=/config/.ssh/komp_known_hosts \
  -o StrictHostKeyChecking=accept-new <user>@<host> shutdown

# отмена
ssh -i /config/.ssh/komp_ed25519 \
  -o BatchMode=yes -o ConnectTimeout=5 \
  -o UserKnownHostsFile=/config/.ssh/komp_known_hosts \
  -o StrictHostKeyChecking=accept-new <user>@<host> cancel

# блокировка (задача \Home\KompLock должна быть создана заранее)
ssh -i /config/.ssh/komp_ed25519 \
  -o BatchMode=yes -o ConnectTimeout=5 \
  -o UserKnownHostsFile=/config/.ssh/komp_known_hosts \
  -o StrictHostKeyChecking=accept-new <user>@<host> lock
```

Из HA: Developer Tools → Actions:

- `shell_command.komp_shutdown` — выключение через минуту;
- `shell_command.komp_cancel_shutdown` — отмена;
- `shell_command.komp_lock` — блокировка экрана;
- `script.komp_lock` / `script.komp_unlock` — блокировка и разблокировка с
  проверкой, что ПК в сети (кнопки `button.komp_lock`/`button.komp_unlock`);
- `wake_on_lan.send_magic_packet` с `mac` — включение;
- `switch.komp` — on/off как обычный переключатель;
- голос: «включи комп», «выключи комп», «отмени выключение компа»,
  «заблокируй комп», «разблокируй комп», «комп включён».

Любая другая команда по SSH должна вернуть ошибку и код 1 — это ожидаемо.
