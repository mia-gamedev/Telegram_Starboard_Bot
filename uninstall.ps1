#Requires -RunAsAdministrator
# Stops the starboard bot and removes its background task. The bot's folder is left as is.
# Run it from an Administrator PowerShell in this folder:
#   powershell -ExecutionPolicy Bypass -File .\uninstall.ps1

$taskName = "StarboardBot"

if (-not (Get-ScheduledTask $taskName -ErrorAction SilentlyContinue)) {
    Write-Host "There is no '$taskName' task, so there is nothing to remove."
    return
}
Stop-ScheduledTask $taskName -ErrorAction Stop
Unregister-ScheduledTask $taskName -Confirm:$false -ErrorAction Stop
Write-Host "Removed: the bot is stopped and '$taskName' no longer starts with Windows."
Write-Host "Delete this folder to remove the rest (.venv, .env and the bot's saved data)."
