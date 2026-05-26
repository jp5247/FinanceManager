# Configures the current PowerShell session for FinanceManager dev work.
# Idempotent — safe to dot-source multiple times.
#
# Usage from any new PowerShell window:
#   . .\scripts\dev-env.ps1            # sets up env in *this* shell
#   npm run tauri dev                  # then run anything
#
# Or as a one-shot wrapper:
#   .\scripts\dev.ps1                  # sets up env + runs `npm run tauri dev`

$ErrorActionPreference = 'Stop'

# 1. Ensure ~/.cargo/bin is on PATH for this session.
$cargoBin = Join-Path $env:USERPROFILE '.cargo\bin'
if (Test-Path $cargoBin) {
    if ($env:PATH -notlike "*$cargoBin*") {
        $env:PATH = "$cargoBin;$env:PATH"
    }
} else {
    Write-Warning "Rust not found at $cargoBin. Run 'winget install Rustlang.Rustup' first."
}

# 2. Source MSVC env (link.exe + Windows SDK lib/include paths).
# Required because our VS BuildTools install has a broken COM registration,
# so cargo's auto-detection can't find MSVC without help.
$vsInstaller = 'C:\Program Files (x86)\Microsoft Visual Studio\Installer'
$vcvars      = 'C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Auxiliary\Build\vcvarsall.bat'

if (-not (Test-Path $vcvars)) {
    Write-Warning "vcvarsall.bat not found at $vcvars. MSVC linking will fail."
} elseif ($env:VSINSTALLDIR) {
    Write-Host 'MSVC env already loaded.' -ForegroundColor DarkGray
} else {
    # vcvarsall.bat calls vswhere.exe unqualified — make sure the Installer
    # dir is on PATH for the cmd.exe child.
    $childPath = if (Test-Path $vsInstaller) { "$vsInstaller;$env:PATH" } else { $env:PATH }
    $cmdLine = 'set "PATH=' + $childPath + '" && "' + $vcvars + '" x64 && set'
    & cmd.exe /c $cmdLine 2>$null | ForEach-Object {
        if ($_ -match '^([^=]+)=(.*)$') {
            [System.Environment]::SetEnvironmentVariable($Matches[1], $Matches[2])
        }
    }
    if ($env:VSINSTALLDIR) {
        $loaded = $env:VSINSTALLDIR
        Write-Host "MSVC env loaded ($loaded)." -ForegroundColor Green
    } else {
        Write-Warning 'vcvarsall.bat ran but VSINSTALLDIR is empty — link step will likely fail.'
    }
}

# 3. Sanity check.
$cargo = (Get-Command cargo -ErrorAction SilentlyContinue).Source
$link  = (Get-Command link.exe -ErrorAction SilentlyContinue).Source
$npm   = (Get-Command npm -ErrorAction SilentlyContinue).Source
Write-Host ''
Write-Host "cargo:    $cargo"
Write-Host "link.exe: $link"
Write-Host "npm:      $npm"
