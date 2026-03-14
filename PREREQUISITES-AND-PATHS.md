# ZODE prerequisites and paths – check report

## Prerequisites (verified)

| Component | Status | Notes |
|-----------|--------|--------|
| **Rust** | OK | rustc 1.94.0, cargo 1.94.0 |
| **Visual Studio Build Tools 2022** | OK | C++ workload, cl.exe found |
| **CMake** | OK | 4.3.0 |
| **Windows SDK** | OK | 10.0.26100.0 under `C:\Program Files (x86)\Windows Kits\10\include` |

## ZODE installation paths

- **Data base dir (Windows):** `%LOCALAPPDATA%\Zode` → `C:\Users\rui vit\AppData\Local\Zode`
- **Single installation:** One shared data location; all profiles/vaults live under that folder.
- **Repo / binary:** Build output is `the-grid\target\debug\zode-bin.exe` (or `release\` for release builds).

If `LOCALAPPDATA` is unset (unusual on Windows), the app falls back to `.zode` in the current working directory (see `profile::base_dir()` in `crates/zode-bin/src/profile.rs`).

## Fixes applied

1. **build-zode.ps1** – Paths were hardcoded for "Visual Studio 18 Community", which you don’t have. The script now:
   - Uses `vswhere` to find the active VS/Build Tools install and MSVC version.
   - Falls back to Build Tools 2022 and a known MSVC path if needed.
   - Discovers the Windows Kits version if 10.0.26100.0 is missing.
   So RocksDB/bindgen should work with your current Build Tools install.

2. **app.rs** – When vault persist fails in `update()`, the code now clears `pending_keypair_persist` in the error branch so the operation is not retried every frame (avoids log spam and wasted work).

## Things to watch

- **Developer PowerShell:** The Start Menu shortcut may still point at the old "Community" install. Use the script `Launch-DevPowerShell-BuildTools.ps1` in this repo (or the Build Tools entry in Start Menu) so the dev environment matches your install.
- **RocksDB LOCK:** If you see "lock file in use" or similar, close all ZODE windows and any `zode-bin` processes; delete `profiles\<id>\data\LOCK` only when no process is using it (or reboot).
- **Vault persist:** If you disabled or removed automatic keypair persist, use **Identity → Update Vault** after the ZODE is running to save the keypair to the vault.

## Quick build

From the repo root (normal PowerShell or after loading Build Tools dev env):

```powershell
.\build-zode.ps1
```

Run the app:

```powershell
cargo run -p zode-bin
# or
.\target\debug\zode-bin.exe
```
