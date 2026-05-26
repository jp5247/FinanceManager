# One-shot wrapper: sets up the dev env and launches `npm run tauri dev`.
# Usage from any new PowerShell window:
#   .\scripts\dev.ps1

$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\dev-env.ps1"
Set-Location "$PSScriptRoot\.."
npm run tauri dev
