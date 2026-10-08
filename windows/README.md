# Stats for Windows

Rust implementation of the Stats system monitor for Windows 10/11 x64.

Download `Stats-Windows-x86_64.zip` from this fork's GitHub Releases, extract it,
and run `stats-windows.exe`. No installation or administrator access is required.
The executable is unsigned. The original macOS app remains in the repository.

## Features

- CPU utilization, per-core usage, and two-minute history
- Memory usage and history
- Disk capacity and used space
- Network receive/send rates and per-interface totals
- Battery charge and status when available
- Top 30 processes by CPU, with name filtering and memory usage
- System tray icon with live metrics tooltip and Show Stats / Quit menu

Close the dashboard to keep monitoring in the tray. Use the tray menu to reopen
the dashboard or exit. Sampling runs once per second; CPU and network rates need
an initial sample interval. Process CPU percentages use 100% per logical core.
Network totals are the counters supplied by Windows for each interface.

This first Windows release does not implement GPU monitoring, hardware sensors,
fan control, Bluetooth, multiple timezone clocks, remote monitoring, automatic
updates, or launch at login. It does not install a driver or contact a server.

## Build and validate

Install stable Rust and the Visual Studio C++ build tools with the Windows SDK.

```powershell
cd windows
cargo build --release --locked
cargo test --locked
cargo clippy --locked --all-targets -- -D warnings
.\target\release\stats-windows.exe --self-test
```

`--self-test` samples real Windows metrics without opening the GUI and exits
nonzero on failure. Release binaries use the Windows GUI subsystem.

Publish by pushing a `windows-v*` tag; the Windows release workflow validates,
builds, packages the executable with this README and the MIT license, and uploads
the ZIP and SHA-256 checksum to GitHub Releases.
