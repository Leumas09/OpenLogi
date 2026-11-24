<#
.SYNOPSIS
  Build this fork and install it for the current user, in the same layout as
  the official MSI (%LOCALAPPDATA%\Programs\OpenLogi).

.DESCRIPTION
  - OpenLogi.exe (GUI), openlogi-agent.exe and openlogi-overlay.exe side by
    side: the agent finds the GUI and the overlay next to itself.
  - bin\openlogi.exe (CLI), in its own folder because Windows paths are
    case-insensitive and openlogi.exe would overwrite OpenLogi.exe.
  - A Start menu shortcut, and bin\ on the user PATH.
  The agent itself keeps HKCU\...\Run\OpenLogiAgent pointed at its own path
  when launch_at_login is on, so starting it from the install folder is
  enough to fix autostart.

.PARAMETER NoBuild
  Install the binaries already in target\release without rebuilding.

.PARAMETER NoStart
  Do not start the agent after installing.
#>
[CmdletBinding()]
param(
    [switch]$NoBuild,
    [switch]$NoStart
)

$ErrorActionPreference = 'Stop'
$repo = Split-Path -Parent $PSScriptRoot
$release = Join-Path $repo 'target\release'
$dest = Join-Path $env:LOCALAPPDATA 'Programs\OpenLogi'
$bin = Join-Path $dest 'bin'

if (-not $NoBuild) {
    $cargo = Get-Command cargo -ErrorAction SilentlyContinue
    if (-not $cargo) {
        $cargo = Join-Path $env:USERPROFILE '.cargo\bin\cargo.exe'
        if (-not (Test-Path $cargo)) { throw 'cargo not found (install rustup first).' }
    }
    Write-Host '==> cargo build --release'
    & $cargo build --release --manifest-path (Join-Path $repo 'Cargo.toml') `
        -p openlogi -p openlogi-agent -p openlogi-desktop -p openlogi-overlay
    if ($LASTEXITCODE -ne 0) { throw "cargo build failed ($LASTEXITCODE)" }
}

$sources = @{
    'openlogi-desktop.exe' = Join-Path $dest 'OpenLogi.exe'
    'openlogi-agent.exe'   = Join-Path $dest 'openlogi-agent.exe'
    'openlogi-overlay.exe' = Join-Path $dest 'openlogi-overlay.exe'
    'openlogi.exe'         = Join-Path $bin 'openlogi.exe'
}
foreach ($name in $sources.Keys) {
    if (-not (Test-Path (Join-Path $release $name))) { throw "missing $release\$name (build first)" }
}

Write-Host '==> stopping running OpenLogi processes'
# Process names are matched case-sensitively so the CLI (openlogi) is not
# confused with the GUI (OpenLogi); both are stopped anyway.
Get-Process -ErrorAction SilentlyContinue |
    Where-Object { $_.ProcessName -in @('OpenLogi', 'openlogi-desktop', 'openlogi-agent', 'openlogi-overlay') } |
    Stop-Process -Force -Confirm:$false
Start-Sleep -Milliseconds 800

Write-Host "==> installing to $dest"
New-Item -ItemType Directory -Force -Path $bin | Out-Null
foreach ($name in $sources.Keys) {
    Copy-Item -Force (Join-Path $release $name) $sources[$name]
}

$startMenu = Join-Path $env:APPDATA 'Microsoft\Windows\Start Menu\Programs\OpenLogi.lnk'
$shell = New-Object -ComObject WScript.Shell
$link = $shell.CreateShortcut($startMenu)
$link.TargetPath = Join-Path $dest 'OpenLogi.exe'
$link.WorkingDirectory = $dest
$link.Save()

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
if (-not (($userPath -split ';') -contains $bin)) {
    [Environment]::SetEnvironmentVariable('Path', ($userPath.TrimEnd(';') + ';' + $bin), 'User')
    Write-Host "==> added $bin to the user PATH"
}

if (-not $NoStart) {
    Write-Host '==> starting the agent'
    Start-Process (Join-Path $dest 'openlogi-agent.exe') -WindowStyle Hidden
}

$commit = (git -C $repo rev-parse --short HEAD) 2>$null
Write-Host "OpenLogi (fork $commit) installed."
