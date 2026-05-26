# One-shot wrapper: sets up the dev env and runs `npm run tauri build`.
# Produces a signed installer in src-tauri\target\release\bundle\.
# Usage from any new PowerShell window:
#   .\scripts\build.ps1

$ErrorActionPreference = 'Stop'
. "$PSScriptRoot\dev-env.ps1"
Set-Location "$PSScriptRoot\.."
npm run tauri build
