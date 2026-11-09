#requires -Version 5.1
<#
    komp-power.ps1 - forced-command wrapper for the home desktop («комп»).

    Home Assistant connects over SSH with an authorized_keys line that forces
    exactly this script:

        restrict,command="powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\ProgramData\Home\komp-power.ps1" ssh-ed25519 AAAA... ha-komp

    OpenSSH puts the client's requested command into SSH_ORIGINAL_COMMAND.
    Only three exact, case-sensitive words are accepted; everything else is
    refused and no shell, file or process beyond shutdown.exe and the fixed
    \\Home\\KompLock scheduled task is touched.

    `lock` cannot call LockWorkStation directly: this script runs in the SSH
    service session, not the interactive desktop. It starts the pre-created
    task `\\Home\\KompLock` instead; that task runs as the logged-on user and
    executes rundll32.exe user32.dll,LockWorkStation in their session
    (see install-lock-task.ps1).

    Keep this file saved as UTF-8 with BOM: Windows PowerShell 5.1 reads .ps1
    without a BOM in the legacy ANSI code page and would mangle the Russian
    message shown in the Windows shutdown dialog.
#>
Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$requested = [string]::Empty
if ($null -ne $env:SSH_ORIGINAL_COMMAND) {
    $requested = $env:SSH_ORIGINAL_COMMAND.Trim()
}

if ($requested -ceq 'shutdown') {
    # Windows shows the countdown and still allows `shutdown /a` / our "cancel".
    & shutdown.exe /s /t 60 /c 'Рикардо: выключение через минуту'
    exit $LASTEXITCODE
}

if ($requested -ceq 'cancel') {
    & shutdown.exe /a
    exit $LASTEXITCODE
}

if ($requested -ceq 'lock') {
    # The task is the only thing that can run LockWorkStation in the
    # interactive session; the name and path are fixed, not client input.
    & schtasks.exe /run /tn '\Home\KompLock'
    exit $LASTEXITCODE
}

[Console]::Error.WriteLine('komp-power: only "shutdown", "cancel" and "lock" are allowed')
exit 1
