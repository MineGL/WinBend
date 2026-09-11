# Builds the release binary and zips a buyer-ready package into dist\.
# Usage:  powershell -ExecutionPolicy Bypass -File dist\package.ps1
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root
cargo build --release
if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
$version = (Select-String -Path Cargo.toml -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$stage = Join-Path $root "dist\stage"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force $stage | Out-Null
Copy-Item "target\release\winbend.exe" $stage
Copy-Item "dist\README.txt" $stage
Copy-Item "LICENSE" $stage
$zip = Join-Path $root "dist\WinBend-$version-win64.zip"
if (Test-Path $zip) { Remove-Item -Force $zip }
Compress-Archive -Path "$stage\*" -DestinationPath $zip
Remove-Item -Recurse -Force $stage
Write-Host "wrote $zip"
