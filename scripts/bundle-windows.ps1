# Build the Windows package: target/Grubcraft-Windows.zip, holding a
# Grubcraft folder with grubcraft.exe and "Read me.txt".
# The exe embeds the game data, models and sounds, so it needs no other files.
# Run from anywhere on Windows: ./scripts/bundle-windows.ps1
$ErrorActionPreference = 'Stop'
Set-Location (Join-Path $PSScriptRoot '..')

cargo build --release -p gc_game
if ($LASTEXITCODE -ne 0) { throw 'cargo build failed' }

$stage = 'target/windows-package'
$dir = Join-Path $stage 'Grubcraft'
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Path $dir | Out-Null

Copy-Item 'target/release/grubcraft.exe' $dir
# Notepad on older Windows wants CRLF line endings.
$text = (Get-Content 'packaging/readme-windows.txt') -join "`r`n"
Set-Content -Path (Join-Path $dir 'Read me.txt') -Value $text -Encoding UTF8

$zip = 'target/Grubcraft-Windows.zip'
if (Test-Path $zip) { Remove-Item -Force $zip }
# Zipping the folder itself keeps "Grubcraft/" as the top level of the zip.
Compress-Archive -Path $dir -DestinationPath $zip
Write-Host "Built $zip"
