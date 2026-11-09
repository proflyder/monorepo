#Requires -Version 5.1
<#
    install-lock-task.ps1 - create the on-demand task that locks the desktop.

    komp-power.ps1 runs in the OpenSSH service session, where
    LockWorkStation would affect the wrong session (or fail). The SSH wrapper
    therefore only starts the task \Home\KompLock with `schtasks /run`.

    This script registers that task once, as the interactive user:

      action    rundll32.exe user32.dll,LockWorkStation
      principal LogonType Interactive ("run only when the user is logged on")
      trigger   none: it runs only on demand from komp-power.ps1

    Run it once from an elevated PowerShell in the target user's session:

      powershell -NoProfile -ExecutionPolicy Bypass -File install-lock-task.ps1

    Re-running it replaces the task (safe, idempotent). The task name and path
    match the fixed strings in komp-power.ps1; keep them in sync.
#>
Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$TaskName = 'KompLock'
$TaskPath = '\Home\'
$CurrentUser = "$env:USERDOMAIN\$env:USERNAME"
$Rundll = Join-Path $env:SystemRoot 'System32\rundll32.exe'

$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principalCheck = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principalCheck.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    throw 'Run this script from an elevated (Administrator) PowerShell.'
}

$action = New-ScheduledTaskAction -Execute $Rundll -Argument 'user32.dll,LockWorkStation'
# Interactive + no password: the task is available only while this user has an
# interactive logon session, which is exactly when a desktop lock makes sense.
$principal = New-ScheduledTaskPrincipal -UserId $CurrentUser -LogonType Interactive -RunLevel Limited
$settings = New-ScheduledTaskSettingsSet `
    -AllowStartIfOnBatteries `
    -DontStopIfGoingOnBatteries `
    -MultipleInstances IgnoreNew `
    -ExecutionTimeLimit (New-TimeSpan -Minutes 1)

Register-ScheduledTask `
    -TaskName $TaskName `
    -TaskPath $TaskPath `
    -Action $action `
    -Principal $principal `
    -Settings $settings `
    -Description 'Lock the interactive desktop for Home Assistant (komp lock).' `
    -Force | Out-Null

Write-Output "Registered $TaskPath$TaskName for $CurrentUser (interactive logon only)."
