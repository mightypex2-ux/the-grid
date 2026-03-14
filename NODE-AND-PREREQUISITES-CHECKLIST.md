# ZODE node & prerequisites checklist

Run this checklist to verify your environment and node setup. Re-run after changing VS, Rust, or the repo.

---

## Prerequisites

- [x] **Rust** — rustc 1.94.0 (or compatible)
- [x] **Cargo** — cargo 1.94.0
- [x] **Visual Studio Build Tools** — 2022 with C++ workload
- [x] **cl.exe** — MSVC compiler found under Build Tools
- [x] **CMake** — installed and on PATH (e.g. 4.3.0)
- [x] **Windows SDK** — `C:\Program Files (x86)\Windows Kits\10\include` exists
- [x] **LOCALAPPDATA** — set (Zode uses it for data dir)

---

## Repo & build

- [x] **the-grid repo** — `Cargo.toml` and `crates\zode-bin` present
- [x] **build-zode.ps1** — present; uses vswhere + MSVC/Windows Kits paths
- [x] **zode-bin built** — `target\debug\zode-bin.exe` (or release) exists
- [x] **Launch-DevPowerShell-BuildTools.ps1** — present (optional; use if VS dev shell points to wrong install)

---

## ZODE data & node

- [x] **Zode data dir** — `%LOCALAPPDATA%\Zode` exists
- [x] **profiles.json** — exists under Zode data dir
- [x] **Profiles** — 1 profile configured
- [ ] **RocksDB LOCK files** — 33 LOCK files under Zode dir (only an issue if ZODE fails to start; close all ZODE instances and delete `profiles\<id>\data\LOCK` if one instance is stuck)

---

## Optional / network

- [ ] **Public address** — If you want others to reach you: set in Settings → General and forward UDP 3690 on your router
- [ ] **Allow private addresses** — For same-LAN peers: Settings → Discovery → Allow private/LAN addresses
- [ ] **Firewall** — Allow zode-bin.exe and UDP 3690 if you need inbound connections

---

## Quick commands

```powershell
# Build (from the-grid root)
.\build-zode.ps1

# Run
cargo run -p zode-bin
# or
.\target\debug\zode-bin.exe
```

---

*Generated from environment check. Re-run the project’s check script or this process to refresh.*
