#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Удаляет провайдер входа Home с ПК «комп».

.DESCRIPTION
    Сначала снимает обе регистрации в реестре (LogonUI перестаёт загружать
    провайдера сразу, без перезагрузки), затем удаляет DLL и секрет. Логи
    остаются на месте.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File uninstall.ps1
#>
[CmdletBinding()]
param(
    [switch]$KeepSecret
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$ProviderClsid = '{9459CCEA-4CCC-4435-89BF-E0E132E0C27F}'
$DllName = 'HomeKompLogon.dll'
$InstallDir = 'C:\Program Files\Home\KompLogon'
$DataDir = 'C:\ProgramData\Home\KompLogon'
$ClassesKey = "HKLM:\SOFTWARE\Classes\CLSID\$ProviderClsid"
$CredentialProvidersKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Authentication\Credential Providers'
$ProviderKey = "$CredentialProvidersKey\$ProviderClsid"

# --- deregister first: takes effect for the next logon screen ----------------
if (Test-Path -LiteralPath $ProviderKey) {
    Remove-Item -LiteralPath $ProviderKey -Recurse -Force
    Write-Output "Removed $ProviderKey"
} else {
    Write-Output "Not registered: $ProviderKey"
}

if (Test-Path -LiteralPath $ClassesKey) {
    Remove-Item -LiteralPath $ClassesKey -Recurse -Force
    Write-Output "Removed $ClassesKey"
} else {
    Write-Output "Not registered: $ClassesKey"
}

# --- then the files -----------------------------------------------------------
$installedDll = Join-Path $InstallDir $DllName
if (Test-Path -LiteralPath $installedDll) {
    # A running LogonUI may hold the image; it is not in the way of logon.
    try {
        Remove-Item -LiteralPath $installedDll -Force -ErrorAction Stop
        Write-Output "Removed $installedDll"
    } catch {
        Write-Warning "Could not delete $installedDll (in use?). It is unregistered and inert."
    }
}

if (-not $KeepSecret) {
    $secretPath = Join-Path $DataDir 'secret.bin'
    if (Test-Path -LiteralPath $secretPath) {
        Remove-Item -LiteralPath $secretPath -Force
        Write-Output "Removed $secretPath"
    }
}

# Configuration (ServiceUrl/Machine/UserName/Domain) stays so a reinstall keeps
# the same values. Logs stay for diagnostics.
Write-Output 'Uninstalled. Logs kept in C:\ProgramData\Home\KompLogon.'
