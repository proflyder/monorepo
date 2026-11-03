#requires -Version 5.1
<#
    komp-power.ps1 - forced-command wrapper for the home desktop («комп»).

    Home Assistant connects over SSH with an authorized_keys line that forces
    exactly this script:

        restrict,command="powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\ProgramData\ricardo\komp-power.ps1" ssh-ed25519 AAAA... ha-komp

    OpenSSH puts the client's requested command into SSH_ORIGINAL_COMMAND.
    Only two exact, case-sensitive words are accepted; everything else is
    refused and no shell, file or process beyond shutdown.exe is touched.

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

[Console]::Error.WriteLine('komp-power: only "shutdown" and "cancel" are allowed')
exit 1
