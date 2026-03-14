# Developer PowerShell for Visual Studio 2022 Build Tools
# Use this if "Developer PowerShell for VS" errors (pointing at uninstalled Community).
# In a normal PowerShell: cd to the-grid, then . .\Launch-DevPowerShell-BuildTools.ps1

$buildToolsPath = "${env:ProgramFiles(x86)}\Microsoft Visual Studio\2022\BuildTools"
$toolsPath = "$buildToolsPath\Common7\Tools"
$launchScript = "$toolsPath\Launch-VsDevShell.ps1"

if (-not (Test-Path $launchScript)) {
    Write-Host "Build Tools not found at: $buildToolsPath" -ForegroundColor Red
    return
}

. $launchScript -VsInstallationPath $buildToolsPath -Arch amd64
Write-Host "Build Tools 2022 dev environment loaded (x64)." -ForegroundColor Green
