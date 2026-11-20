#requires -Version 5.1
Set-StrictMode -Version 2.0
$ErrorActionPreference = 'Stop'

$requested = [string]::Empty
if ($null -ne $env:SSH_ORIGINAL_COMMAND) {
    $requested = $env:SSH_ORIGINAL_COMMAND.Trim()
}

if ($requested -ceq 'shutdown') {
    & shutdown.exe /s /t 60 /c 'Рикардо: выключение через минуту'
    exit $LASTEXITCODE
}

if ($requested -ceq 'cancel') {
    & shutdown.exe /a
    exit $LASTEXITCODE
}

if ($requested -ceq 'lock') {
    & schtasks.exe /run /tn '\Home\KompLock'
    exit $LASTEXITCODE
}

if ($requested -ceq 'status') {
    $shellSessions = @(
        Get-Process -Name explorer -ErrorAction SilentlyContinue |
            ForEach-Object { $_.SessionId }
    )
    $logonSessions = @(
        Get-Process -Name LogonUI -ErrorAction SilentlyContinue |
            ForEach-Object { $_.SessionId }
    )
    $locked = $false
    if ($logonSessions.Count -gt 0) {
        if (-not $shellSessions) {
            $locked = $true
        }
        foreach ($session in $logonSessions) {
            if ($shellSessions -contains $session) {
                $locked = $true
            }
        }
    }
    if ($locked) {
        'locked'
    } else {
        'unlocked'
    }
    exit 0
}

[Console]::Error.WriteLine('komp-power: only "shutdown", "cancel", "lock" and "status" are allowed')
exit 1
