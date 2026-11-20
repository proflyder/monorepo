#Requires -RunAsAdministrator
[CmdletBinding()]
param(
    [System.Security.SecureString]$Password,
    [string]$Key
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$DataDir = 'C:\ProgramData\Home\KompLogon'
$SecretPath = Join-Path $DataDir 'secret.bin'
$Entropy = 'home-komp-logon-v1'

try {
    Add-Type -AssemblyName System.Security -ErrorAction Stop
} catch {
}

function Set-AclStrict {
    param([string]$Path, [string[]]$Arguments)
    & icacls.exe $Path @Arguments | Out-Null
    if ($LASTEXITCODE -ne 0) {
        throw "icacls.exe failed for $Path (exit $LASTEXITCODE)"
    }
}

if (-not (Test-Path -LiteralPath $DataDir -PathType Container)) {
    New-Item -ItemType Directory -Path $DataDir -Force | Out-Null
    Set-AclStrict -Path $DataDir -Arguments @(
        '/inheritance:r',
        '/grant:r', '*S-1-5-18:(OI)(CI)F',
        '/grant:r', '*S-1-5-32-544:(OI)(CI)F'
    )
}

if ($null -eq $Password) {
    $Password = Read-Host -AsSecureString -Prompt 'Пароль Windows для учетной записи входа'
}

if ([string]::IsNullOrWhiteSpace($Key)) {
    if (-not [Console]::IsInputRedirected) {
        throw 'Key is required: pass -Key <base64> or pipe one line into stdin'
    }
    $Key = [Console]::In.ReadLine()
}

$keyBytes = $null
$plainPassword = $null
$payload = $null
$bstr = [IntPtr]::Zero
try {
    if ($null -eq $Key) { throw 'Key is required (parameter or stdin)' }
    try {
        $keyBytes = [Convert]::FromBase64String($Key.Trim())
    } catch {
        throw 'Key must be base64'
    }
    if ($keyBytes.Length -ne 32) {
        throw "Key must decode to 32 bytes (got $($keyBytes.Length))"
    }

    $bstr = [Runtime.InteropServices.Marshal]::SecureStringToBSTR($Password)
    $plainPassword = [Runtime.InteropServices.Marshal]::PtrToStringBSTR($bstr)
    if ([string]::IsNullOrEmpty($plainPassword)) {
        throw 'Password must not be empty'
    }

    $payload = [ordered]@{
        version  = 1
        password = $plainPassword
        key      = [Convert]::ToBase64String($keyBytes)
    } | ConvertTo-Json -Compress

    $entropyBytes = [Text.Encoding]::UTF8.GetBytes($Entropy)
    $plainBytes = [Text.Encoding]::UTF8.GetBytes($payload)
    try {
        $blob = [System.Security.Cryptography.ProtectedData]::Protect(
            $plainBytes, $entropyBytes, [System.Security.Cryptography.DataProtectionScope]::LocalMachine)
    } finally {
        [Array]::Clear($plainBytes, 0, $plainBytes.Length)
    }

    [IO.File]::WriteAllBytes($SecretPath, $blob)

    Set-AclStrict -Path $SecretPath -Arguments @(
        '/inheritance:r',
        '/grant:r', '*S-1-5-18:F',
        '/grant:r', '*S-1-5-32-544:F'
    )

    Write-Output "Secret written to $SecretPath (DPAPI LocalMachine, entropy $Entropy)."
    Write-Output 'Password and key were not printed.'
} finally {
    if ($bstr -ne [IntPtr]::Zero) {
        [Runtime.InteropServices.Marshal]::ZeroFreeBSTR($bstr)
    }
    $plainPassword = $null
    $payload = $null
    if ($keyBytes -ne $null) { [Array]::Clear($keyBytes, 0, $keyBytes.Length) }
}
