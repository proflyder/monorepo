#Requires -RunAsAdministrator
<#
.SYNOPSIS
    Устанавливает провайдер входа Ricardo для ПК «комп».

.DESCRIPTION
    Копирует DLL, настраивает ACL, пишет конфигурацию в реестр и регистрирует
    CLSID провайдера. Не трогает другие провайдеры, не регистрирует фильтры и
    не включает автоматический вход сам по себе: плитка появляется только
    после одноразового гранта.

    Пароль и ключ пишутся отдельно через set-secret.ps1.

    ServiceUrl, Machine и UserName обязательны: значения задаёт проект при
    установке, в самом решении их нет.

.EXAMPLE
    powershell -NoProfile -ExecutionPolicy Bypass -File install.ps1 `
        -ServiceUrl 'http://192.0.2.10:8080' `
        -Machine '<machine>' -UserName '<user>'
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$ServiceUrl,
    [Parameter(Mandatory = $true)]
    [string]$Machine,
    [Parameter(Mandatory = $true)]
    [string]$UserName,
    [string]$Domain = '.',
    [string]$SourceDir = (Join-Path $PSScriptRoot 'out')
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$ProviderClsid = '{9459CCEA-4CCC-4435-89BF-E0E132E0C27F}'
$ProviderName = 'Ricardo komp logon provider'
$DllName = 'RicardoKompLogon.dll'
$ConfigKey = 'HKLM:\SOFTWARE\Ricardo\KompLogon'
$InstallDir = 'C:\Program Files\Ricardo\KompLogon'
$DataDir = 'C:\ProgramData\Ricardo\KompLogon'
$ClassesKey = "HKLM:\SOFTWARE\Classes\CLSID\$ProviderClsid"
$InprocKey = "$ClassesKey\InprocServer32"
$CredentialProvidersKey = 'HKLM:\SOFTWARE\Microsoft\Windows\CurrentVersion\Authentication\Credential Providers'
$ProviderKey = "$CredentialProvidersKey\$ProviderClsid"

function Set-AclStrict {
    param([string]$Path, [string[]]$Arguments)
    & icacls.exe $Path @Arguments | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "icacls.exe failed for $Path (exit $LASTEXITCODE)"
    }
}

# --- validate input -----------------------------------------------------------
if ($Machine -notmatch '^[a-z0-9-]{1,32}$') {
    throw "Machine must match ^[a-z0-9-]{1,32}$"
}
if ($ServiceUrl -notmatch '^https?://[A-Za-z0-9._:\[\]-]+(/[^?#\s]*)?$') {
    throw "ServiceUrl must look like http://host:port (no userinfo, query or fragment)"
}
if ([string]::IsNullOrWhiteSpace($UserName) -or $UserName.Length -gt 256) {
    throw 'UserName must be a non-empty account name'
}
if ([string]::IsNullOrWhiteSpace($Domain) -or $Domain.Length -gt 256) {
    throw 'Domain must be a non-empty domain or "."'
}

$sourceDll = Join-Path $SourceDir $DllName
if (-not (Test-Path -LiteralPath $sourceDll -PathType Leaf)) {
    throw "DLL not found: $sourceDll (build it first with build.sh)"
}

# --- copy the DLL -------------------------------------------------------------
New-Item -ItemType Directory -Path $InstallDir -Force | Out-Null
$installedDll = Join-Path $InstallDir $DllName
try {
    Copy-Item -LiteralPath $sourceDll -Destination $installedDll -Force -ErrorAction Stop
} catch {
    throw ("Could not replace $installedDll : $($_.Exception.Message). " +
           'Log off (so LogonUI is not running) and retry, or run uninstall.ps1 first.')
}

# SYSTEM and Administrators may write, Users only read/execute.
Set-AclStrict -Path $InstallDir -Arguments @(
    '/inheritance:r',
    '/grant:r', '*S-1-5-18:(OI)(CI)F',
    '/grant:r', '*S-1-5-32-544:(OI)(CI)F',
    '/grant:r', '*S-1-5-32-545:(OI)(CI)RX'
)

# --- data directory (secret + log) -------------------------------------------
New-Item -ItemType Directory -Path $DataDir -Force | Out-Null
Set-AclStrict -Path $DataDir -Arguments @(
    '/inheritance:r',
    '/grant:r', '*S-1-5-18:(OI)(CI)F',
    '/grant:r', '*S-1-5-32-544:(OI)(CI)F'
)

# --- configuration ------------------------------------------------------------
New-Item -Path $ConfigKey -Force | Out-Null
Set-ItemProperty -Path $ConfigKey -Name 'ServiceUrl' -Value $ServiceUrl -Type String
Set-ItemProperty -Path $ConfigKey -Name 'Machine' -Value $Machine -Type String
Set-ItemProperty -Path $ConfigKey -Name 'UserName' -Value $UserName -Type String
Set-ItemProperty -Path $ConfigKey -Name 'Domain' -Value $Domain -Type String

# Reinstall is the documented way to re-enable after three failed logons.
Set-ItemProperty -Path $ConfigKey -Name 'FailCount' -Value 0 -Type DWord
Set-ItemProperty -Path $ConfigKey -Name 'Disabled' -Value 0 -Type DWord

# --- COM registration ---------------------------------------------------------
New-Item -Path $ClassesKey -Force | Out-Null
Set-ItemProperty -Path $ClassesKey -Name '(default)' -Value $ProviderName -Type String
New-Item -Path $InprocKey -Force | Out-Null
Set-ItemProperty -Path $InprocKey -Name '(default)' -Value $installedDll -Type String
Set-ItemProperty -Path $InprocKey -Name 'ThreadingModel' -Value 'Apartment' -Type String

# --- LogonUI registration (credential provider, never a filter) --------------
New-Item -Path $ProviderKey -Force | Out-Null
Set-ItemProperty -Path $ProviderKey -Name '(default)' -Value $ProviderName -Type String

Write-Output 'Ricardo komp logon provider installed.'
Write-Output "  CLSID:      $ProviderClsid"
Write-Output "  DLL:        $installedDll"
Write-Output "  Config:     $ConfigKey"
Write-Output "  ServiceUrl: $ServiceUrl"
Write-Output "  Account:    $Domain\$UserName (machine $Machine)"
Write-Output ''
Write-Output 'Next: set-secret.ps1 writes the DPAPI-protected password and key.'
Write-Output 'Then log off (or lock) and check status.ps1 / logon.log.'
