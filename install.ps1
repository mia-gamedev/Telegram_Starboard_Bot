#Requires -RunAsAdministrator
# Installs the starboard bot as a background task that starts with Windows.
# Run it from an Administrator PowerShell in this folder:
#   powershell -ExecutionPolicy Bypass -File .\install.ps1

$ErrorActionPreference = "Stop"
$taskName = "StarboardBot"
$oldTaskName = "HallOfFameBot"
Set-Location $PSScriptRoot

$python = Get-Command py, python -ErrorAction SilentlyContinue | Select-Object -First 1
if (-not $python) {
    throw "Python is missing. Install Python 3.10+ from https://www.python.org/downloads/windows/ and run this again."
}
& $python.Source -c "import sys; sys.exit(sys.version_info < (3, 10))"
if ($LASTEXITCODE -ne 0) { throw "Python 3.10 or newer is required." }

# The bot saves its setup and settings to .env too, so only the BOT_TOKEN line is touched here.
$envPath = Join-Path $PSScriptRoot ".env"
$envLines = @(if (Test-Path $envPath) { [IO.File]::ReadAllLines($envPath) })
$tokenLine = $envLines | Where-Object { $_ -match '^BOT_TOKEN=' } | Select-Object -First 1
$token = ("$tokenLine" -replace '^BOT_TOKEN=', '').Trim()
while ($token -notmatch '^\d+:[\w-]+$') {
    $token = (Read-Host "Bot token from @BotFather").Trim()
}

# Older Windows Server versions don't enable TLS 1.2 by default, which Telegram requires.
[Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12
try {
    $botInfo = Invoke-RestMethod "https://api.telegram.org/bot$token/getMe"
} catch {
    throw "Checking the bot token with Telegram failed ($($_.Exception.Message)). Fix BOT_TOKEN in .env (or clear it to be asked again), check the internet connection and run this again."
}
$botName = "@$($botInfo.result.username)"
Write-Host "Token OK: $botName"

if ($tokenLine) {
    $envLines = $envLines -replace '^BOT_TOKEN=.*', "BOT_TOKEN=$token"
} else {
    $envLines += "BOT_TOKEN=$token"
}
[IO.File]::WriteAllLines($envPath, [string[]]$envLines)

if (-not (Test-Path .venv)) { & $python.Source -m venv .venv }
& .venv\Scripts\python.exe -m pip install --quiet --disable-pip-version-check -r requirements.txt
if ($LASTEXITCODE -ne 0) { throw "Installing the Python packages failed." }

# The ScheduledTasks cmdlets ignore $ErrorActionPreference, so they get -ErrorAction Stop.
# The bot used to be the "Hall of Fame" bot; two copies on one token would fight over updates.
if (Get-ScheduledTask $oldTaskName -ErrorAction SilentlyContinue) {
    Stop-ScheduledTask $oldTaskName -ErrorAction Stop
    Unregister-ScheduledTask $oldTaskName -Confirm:$false -ErrorAction Stop
}
if (Get-ScheduledTask $taskName -ErrorAction SilentlyContinue) { Stop-ScheduledTask $taskName -ErrorAction Stop }
$pythonExe = Join-Path $PSScriptRoot ".venv\Scripts\python.exe"
$action = New-ScheduledTaskAction -Execute "`"$pythonExe`"" -Argument "starboard_bot.py" -WorkingDirectory $PSScriptRoot
$triggers = (New-ScheduledTaskTrigger -AtStartup), (New-ScheduledTaskTrigger -Once -At (Get-Date) -RepetitionInterval (New-TimeSpan -Minutes 5))
$settings = New-ScheduledTaskSettingsSet -ExecutionTimeLimit 0 -MultipleInstances IgnoreNew
Register-ScheduledTask $taskName -Action $action -Trigger $triggers -Settings $settings -User SYSTEM -Force -ErrorAction Stop | Out-Null
Start-ScheduledTask $taskName -ErrorAction Stop

Write-Host "Installed: '$taskName' runs in the background, starts with Windows and restarts if it stops."
Write-Host "Check it with: Get-ScheduledTask $taskName"
Write-Host "Next: make $botName an admin in the group, then run /setup in the starboard topic and /watch in the music topic."
