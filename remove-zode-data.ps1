# Remove all Zode profiles and data (fresh start).
# Close ALL ZODE windows first, then run: .\remove-zode-data.ps1

$zodeBase = Join-Path $env:LOCALAPPDATA "Zode"

if (-not (Test-Path $zodeBase)) {
    Write-Host "No Zode data at $zodeBase. Already clean."
    exit 0
}

$running = Get-Process -Name "zode-bin" -ErrorAction SilentlyContinue
if ($running) {
    Write-Host "ZODE is still running (PID $($running.Id)). Close all ZODE windows and run this script again."
    exit 1
}

Remove-Item -Path $zodeBase -Recurse -Force
Write-Host "Zode data removed: $zodeBase"
Write-Host "Next time you run ZODE you will start with no profiles (setup screen)."
