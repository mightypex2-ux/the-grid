# Build ZODE on Windows - sets include paths so RocksDB (bindgen) can find MSVC headers.
# Prefer Build Tools 2022; fall back to first MSVC found under any VS install.
$vsPath = $null
if (Test-Path "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe") {
    $vsPath = & "${env:ProgramFiles(x86)}\Microsoft Visual Studio\Installer\vswhere.exe" `
        -latest -products * -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 -property installationPath 2>$null
}
$msvcInclude = $null
if ($vsPath -and (Test-Path "$vsPath\VC\Tools\MSVC")) {
    $msvcDir = Get-ChildItem "$vsPath\VC\Tools\MSVC" -Directory -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($msvcDir -and (Test-Path "$($msvcDir.FullName)\include")) {
        $msvcInclude = "$($msvcDir.FullName)\include"
    }
}
if (-not $msvcInclude) {
    $msvcInclude = "C:\Program Files (x86)\Microsoft Visual Studio\2022\BuildTools\VC\Tools\MSVC\14.44.35207\include"
}
$winKit = "C:\Program Files (x86)\Windows Kits\10\include\10.0.26100.0"
if (-not (Test-Path $winKit)) {
    $winKitBase = "C:\Program Files (x86)\Windows Kits\10\include"
    if (Test-Path $winKitBase) {
        $ver = Get-ChildItem $winKitBase -Directory -ErrorAction SilentlyContinue | Select-Object -First 1
        if ($ver) { $winKit = Join-Path $winKitBase $ver.Name }
    }
}
$env:BINDGEN_EXTRA_CLANG_ARGS = @(
    "-I`"$msvcInclude`"",
    "-I`"$winKit\ucrt`"",
    "-I`"$winKit\um`"",
    "-I`"$winKit\shared`""
) -join ' '
cargo build -p zode-bin @args
