#requires -Version 5.1
<#
    komp-power.ps1 - forced-command wrapper for the home desktop («комп»).

    Home Assistant connects over SSH with an authorized_keys line that forces
    exactly this script:

        restrict,command="powershell.exe -NoProfile -ExecutionPolicy Bypass -File C:\ProgramData\Home\komp-power.ps1" ssh-ed25519 AAAA... ha-komp

    OpenSSH puts the client's requested command into SSH_ORIGINAL_COMMAND.
    Only four exact, case-sensitive words are accepted; everything else is
    refused and no shell, file or process beyond shutdown.exe, the fixed
    \\Home\\KompLock scheduled task and read-only process inspection for
    `status` is touched.

    `status` prints exactly `locked` or `unlocked` and exits 0. The screen is
    locked while LogonUI.exe runs in the user's interactive session: Windows
    starts LogonUI for the lock screen (and for the initial sign-in desktop)
    and stops it after sign-in. The session is identified through explorer.exe,
    the interactive shell; an SSH session (session 0, no explorer) can
    therefore never be mistaken for a locked desktop. With no explorer at all
    the machine is at the sign-in screen, which is locked too.

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

if ($requested -ceq 'status') {
    # Read-only process inspection: no shell, no files, no external program.
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
            # No interactive shell: the machine shows the sign-in screen.
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
