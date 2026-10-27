# Controller Control

Use a game controller as a mouse and media remote on Windows.

## About this project

This is a **vibe-coded project**: it was built through an iterative, AI-assisted coding process. It is a personal project and may contain bugs or rough edges. Please review the code and use it at your own discretion.

## What it does

- Moves the mouse pointer with a controller's analog stick.
- Uses controller buttons for mouse clicks, scrolling, and configurable keyboard shortcuts.
- Provides an on-screen control panel and a system tray icon.
- Includes a Rust implementation (`src/main.rs`) and a Python implementation (`controller_to_pc.py`).

## Build the Rust version

The Rust version is the primary build. It requires Windows and the stable Rust toolchain (Cargo).

```powershell
cargo build --release
```

The executable is written to `target\release\controller_to_pc.exe`.

Alternatively, run `build_rust.bat` to build the release executable and copy it to your Desktop.

## Build the Python version

The Python implementation can be packaged with PyInstaller. `build_exe.bat` installs its Python dependencies and builds a standalone executable; it expects Python 3.12 and copies the result to your Desktop.

## Project layout

- `src/main.rs` — Rust application
- `controller_to_pc.py` — Python application
- `assets/` — application icons and Windows resource file
- `build_rust.bat` / `build_exe.bat` — Windows build scripts

