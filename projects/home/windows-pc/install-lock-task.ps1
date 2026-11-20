#Requires -Version 5.1
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
