#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Показывает состояние провайдера входа Ricardo (без секретов).

.DESCRIPTION
    Печатает конфигурацию, состояние регистрации, DLL, FailCount/Disabled и
    последние строки logon.log. Пароль и ключ не читаются и не печатаются.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File status.ps1
#>
[CmdletBinding()]
param(
    [int]$Tail = 30
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Continue'

$ProviderClsid = '{9459CCEA-4CCC-4435-89BF-E0E132E0C27F}'
$DllName = 'RicardoKompLogon.dll'
$ConfigKey = 'HKLM:\SOFTWARE\Ricardo\KompLogon'
$InstallDir = 'C:\Program Files\Ricardo\KompLogon'
$DataDir = 'C:\ProgramData\Ricardo\KompLogon'
$LogPath = Join-Path $DataDir 'logon.log'
$ClassesKey = "HKLM:\SOFTWARE\Classes\CLSID\$ProviderClsid"
$InprocKey = "$ClassesKey\InprocServer32"
$CredentialProvidersKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Authentication\Credential Providers'
$ProviderKey = "$CredentialProvidersKey\$ProviderClsid"

function Format-Value {
    param([object]$Value, [string]$Missing = '<missing>')
    if ($null -eq $Value) { return $Missing }
    return [string]$Value
}

Write-Output '== configuration =='
if (Test-Path -LiteralPath $ConfigKey) {
    $config = Get-ItemProperty -Path $ConfigKey
    Write-Output ("  ServiceUrl  : {0}" -f (Format-Value $config.ServiceUrl))
    Write-Output ("  Machine     : {0}" -f (Format-Value $config.Machine))
    Write-Output ("  UserName    : {0}" -f (Format-Value $config.UserName))
    Write-Output ("  Domain      : {0}" -f (Format-Value $config.Domain))
    Write-Output ("  FailCount   : {0}" -f (Format-Value $config.FailCount '0'))
    Write-Output ("  Disabled    : {0}" -f (Format-Value $config.Disabled '0'))
} else {
    Write-Output "  $ConfigKey is missing (not installed)"
}

Write-Output ''
Write-Output '== registration =='
$classesPresent = Test-Path -LiteralPath $ClassesKey
Write-Output ("  CLSID key            : {0}" -f $(if ($classesPresent) { 'present' } else { 'MISSING' }))
if ($classesPresent -and (Test-Path -LiteralPath $InprocKey)) {
    $inproc = Get-ItemProperty -Path $InprocKey
    Write-Output ("  InprocServer32       : {0}" -f (Format-Value $inproc.'(default)'))
    Write-Output ("  ThreadingModel       : {0}" -f (Format-Value $inproc.ThreadingModel))
}
Write-Output ("  Credential Providers : {0}" -f $(if (Test-Path -LiteralPath $ProviderKey) { 'present' } else { 'MISSING' }))

$installedDll = Join-Path $InstallDir $DllName
if (Test-Path -LiteralPath $installedDll) {
    $file = Get-Item -LiteralPath $installedDll
    Write-Output ("  DLL                  : {0} ({1} bytes, {2})" -f $file.FullName, $file.Length, $file.LastWriteTime)
} else {
    Write-Output ("  DLL                  : MISSING ({0})" -f $installedDll)
}

$secretPath = Join-Path $DataDir 'secret.bin'
Write-Output ("  Secret blob          : {0}" -f $(if (Test-Path -LiteralPath $secretPath) { 'present' } else { 'MISSING (run set-secret.ps1)' }))

Write-Output ''
Write-Output "== last $Tail log lines =="
if (Test-Path -LiteralPath $LogPath) {
    Get-Content -LiteralPath $LogPath -Tail $Tail
} else {
    Write-Output '  (no log yet)'
}
