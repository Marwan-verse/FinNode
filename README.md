# FinNode

FinNode is a desktop project navigator built with Tauri (Rust backend) and Svelte (frontend).
It gives you a draggable node board window and a separate settings/control window so you can launch project actions, run macros, type text, and send keyboard shortcut sequences quickly.

## Table of Contents

1. Product Overview
2. Core Capabilities
3. Runtime Windows Model
4. Repository Layout
5. Requirements
6. Quick Start
7. Command Reference
8. Usage Guide
9. Macro Actions (Detailed)
10. Keyboard Shortcut Recorder (Detailed)
11. Persistence and Data Model
12. System Tray and Global Hotkeys
13. Platform Input Automation Behavior
14. Build and Packaging
15. Windows Cross-Compile from Linux
16. Troubleshooting
17. Security and Operational Notes
18. Development Notes

## Product Overview

FinNode runs as a desktop app with two windows:

- A settings/control window (main) where you manage workspaces, nodes, and app settings.
- A desktop node board overlay (desktop) where nodes and links are visualized and launched.

Each workspace contains nodes with launch targets and optional macro steps. Node actions can open paths, editors, URLs, scripts, or execute automation steps such as typing text and keyboard shortcuts.

## Core Capabilities

- Multi-workspace node graph management.
- Draggable node board with SVG link rendering.
- Node-level launch targets:
	- path
	- editor command/path
	- browser URL
	- script command or uploaded script file
- Macro engine with sequential steps:
	- run-script
	- run-uploaded-script
	- type-text
	- keyboard-shortcut
	- open-path
	- open-editor
	- open-browser
	- open-application
	- delay (milliseconds)
- Keyboard shortcut recorder with multi-switch sequence support.
- System tray controls and global shortcuts.
- Start-on-boot toggle (enabled by default).
- Desktop window size/position persistence across move, resize, close, app exit, and restart.
- Single-instance process lock.

## Runtime Windows Model

### Main window (settings/control)

- Label: `main`
- Role: settings, workspaces, node editor, status/activity
- Minimize button behavior: hides to tray/taskbar-off state via backend command
- Close button behavior: fully exits app
- Can be reopened from tray menu or tray click

### Desktop window (node board)

- Label: `desktop`
- Transparent overlay-style board, resizable and movable
- Designed to stay low in z-order (platform-specific best effort)
- Supports click-through mode logic (not over interactive node bounds)
- Position and size are persisted and restored

## Repository Layout

```text
.
|- index.html
|- package.json
|- README.md
|- scripts/
|  |- make-executable.sh
|  |- make-windows-executable.sh
|- src/
|  |- App.svelte
|  |- main.js
|  |- style.css
|- src-tauri/
|  |- Cargo.toml
|  |- tauri.conf.json
|  |- src/
|     |- main.rs
|- artifacts/
	 |- windows/
```

## Requirements

## Node and npm

- Node.js with npm available on PATH
- Dependencies are installed via `npm install --include=dev`

## Rust and Cargo

- Rust toolchain required for desktop build/run
- Build scripts auto-install rustup if missing

## Linux native dependencies (for Tauri v1 GTK/WebKit)

On apt-based systems, helper scripts can auto-install required packages. If doing manual install, use:

```bash
sudo apt-get update
sudo apt-get install -y \
	pkg-config \
	libgtk-3-dev \
	libayatana-appindicator3-dev \
	librsvg2-dev \
	libsoup2.4-dev \
	patchelf

# One of the following based on distro availability
sudo apt-get install -y libwebkit2gtk-4.0-dev
# or
sudo apt-get install -y libwebkit2gtk-4.1-dev
```

## Input automation utilities

For automation features on Linux:

- `type-text` action:
	- uses `xdotool` first
	- falls back to `wtype`
- `keyboard-shortcut` action:
	- requires `xdotool`

Recommended install:

```bash
sudo apt-get install -y xdotool wtype
```

## Quick Start

## 1) Install dependencies

```bash
npm install
```

## 2) Run desktop app in dev mode

```bash
npm run dev:desktop
```

This invokes `scripts/make-executable.sh dev`, which handles environment checks and launches `npx tauri dev`.

## 3) Optional web-only preview

```bash
npm run dev
```

Note: desktop runtime uses built assets (`dist`) via Tauri config (`beforeDevCommand` is `npm run build:web`).

## Command Reference

| Command | What it does |
|---|---|
| `npm run dev` | Start Vite web dev server |
| `npm run dev:desktop` | Run desktop app in dev mode through helper script |
| `npm run build:web` | Build web assets into `dist/` |
| `npm run build:exe` | Build desktop bundle through helper script |
| `npm run make` | Alias for `build:exe` |
| `npm run build:windows` | Build Windows executable from Linux helper script |
| `npm run make:windows` | Alias for `build:windows` |

Helper scripts:

- `scripts/make-executable.sh`
	- bootstraps Rust if missing
	- validates Linux deps
	- applies WebKit 4.1 compatibility shim for Tauri v1 when needed
	- installs npm deps
	- runs Tauri dev/build
- `scripts/make-windows-executable.sh`
	- installs Rust windows target `x86_64-pc-windows-gnu` if needed
	- installs MinGW (`gcc-mingw-w64-x86-64`) on apt systems
	- builds web assets
	- compiles `finnode.exe`
	- exports artifacts to `artifacts/windows/` by default

## Usage Guide

## Workspaces

- Create workspace: enter name and click `+`
- Switch workspace: select from dropdown
- Delete workspace: removes active workspace (cannot delete last workspace)

## Nodes

- Add node: `+ Add`
- Layout: `Layout` button
- Search: opens quick launcher
- Clone/Delete nodes from list or context menu
- Main node is locked and cannot be deleted

## Node Actions

Node expanded/context actions may include:

- Open path
- Open editor
- Open browser
- Run script
- Run macro
- Edit node

## Quick Launcher

- Global hotkey can trigger launcher event (`Alt+Space`)
- In-app shortcuts:
	- `Ctrl/Cmd + K`
	- `Alt + Space`
- Enter launches selected result

## Window Controls

- Settings minimize: hide settings to tray
- Settings close: full app exit
- Desktop board can be resized/moved and state is restored at startup

## Macro Actions (Detailed)

Macro steps execute sequentially in the order shown in the editor.

## Action: run-script

- Executes shell command string
- Can optionally use node working directory
- If value points to an existing file, executes file directly

## Action: run-uploaded-script

- Uses node-uploaded script file path
- Requires uploaded script file to exist

## Action: type-text

- Sends keystrokes to active window
- FinNode releases settings focus before macro input actions
- Linux tries `xdotool`, then `wtype`

## Action: keyboard-shortcut

- Sends key combinations/chords to active window
- Supports multi-switch sequence, e.g.:
	- `Ctrl+K, Ctrl+C`
	- `Ctrl+Shift+P`

## Action: open-path

- Opens path or target with OS default opener

## Action: open-editor

- Executes editor target/path fallback

## Action: open-browser

- Opens URL in default browser

## Action: open-application

- Runs command as script/shell command

## Action: delay

- Sleeps for provided milliseconds (default parse fallback: `1000`)

## Keyboard Shortcut Recorder (Detailed)

Recorder behavior in node macro editor:

- Choose macro action `Keyboard shortcut`.
- Click `Record`.
- Press key combination (must include a non-modifier key).
- Press next combo to append sequence (multi-switch chain).
- Click `Stop` or press `Esc` to end recording.

Normalization rules:

- Modifier ordering is normalized to:
	- `Ctrl`, `Alt`, `Shift`, `Meta`
- Chords are separated by commas for sequence execution.
- In editor display, keys are shown with explicit ` + ` separators.

Example stored value:

```text
Ctrl+K, Ctrl+C
```

Example display in editor preview:

```text
Ctrl + K, Ctrl + C
```

## Persistence and Data Model

FinNode persists state in config directory under `FinNode/config.json`.

Typical config base directories:

- Linux: `~/.config/FinNode/`
- Windows: `%APPDATA%/FinNode/`
- macOS: `~/Library/Application Support/FinNode/`

Saved items include:

- active workspace id
- workspace list
- all node definitions per workspace
- node links and targets
- macro definitions
- command history cache
- app settings:
	- `start_on_boot`
	- `desktop_window` geometry

Additional files:

- Uploaded scripts are saved under `FinNode/scripts/`
- Single instance lock file: `FinNode/app.lock`

## Window State Persistence

Desktop board geometry is saved when:

- desktop window is moved
- desktop window is resized
- desktop window close is requested
- full app exit is requested

Desktop board geometry is restored during setup at startup.

## Start on Boot

- UI toggle in Application settings
- Default: enabled
- Backed by `tauri-plugin-autostart` (v1 branch)

## System Tray and Global Hotkeys

Tray menu entries:

- Open Settings
- Toggle Stealth
- Show/Hide Desktop Nodes
- Toggle Background Click-Through
- Exit

Tray interactions:

- Left click/double click opens settings window

Registered global shortcuts:

- `Alt+S`: toggle stealth mode
- `Alt+I`: toggle desktop click-through
- `Alt+Space`: toggle quick launcher

## Platform Input Automation Behavior

## Linux

- `type-text`:
	- `xdotool type --clearmodifiers --delay 1`
	- fallback `wtype`
- `keyboard-shortcut`:
	- `xdotool key --clearmodifiers`
	- requires `xdotool`

## Windows

- Uses PowerShell + `System.Windows.Forms.SendKeys`
- `Meta` (Windows key) is not supported in shortcut sender path

## macOS

- Uses `osascript` + `System Events`
- Supports modifier combinations and common special keys in implementation

## Build and Packaging

Desktop bundle build:

```bash
npm run build:exe
```

Output is generated under Tauri target bundle directories.

Web-only build:

```bash
npm run build:web
```

## Windows Cross-Compile from Linux

Build command:

```bash
npm run make:windows
```

Default exported artifacts:

- `artifacts/windows/finnode.exe`
- `artifacts/windows/WebView2Loader.dll` (if present)

Environment variables:

- `FINNODE_WINDOWS_EXPORT_DIR`:
	- override final artifact export directory
- `FINNODE_WINDOWS_TARGET_DIR`:
	- override cargo target dir for windows builds
- `CARGO_TARGET_DIR`:
	- respected when explicit windows target dir is not set

Notes:

- Script may relocate target dir automatically for `/mnt/*` workspace cases to avoid resource file issues.

## Troubleshooting

## "App closed" but still running

- Settings minimize hides app window to tray.
- Reopen from tray icon/menu.

## Text or shortcut goes to wrong window

- Ensure target window is focused before macro trigger.
- FinNode now hides/releases settings focus before input actions, but OS focus policies can still affect behavior.
- On Linux, verify `xdotool` is installed for shortcut injection.

## Keyboard shortcut action not working on Linux

- Install `xdotool`.
- Verify desktop environment allows synthetic input.

## Type-text not working on Linux

- Install `xdotool` and `wtype`.
- Wayland/X11 security policies can limit synthetic input in some environments.

## WebKit/GTK build failures on Linux

- Ensure required GTK/WebKit dev packages are installed.
- Use helper script to auto-install when on apt-based distro.

## "FinNode is already running"

- Remove stale lock only if app is fully closed:
	- lock path is under config dir: `FinNode/app.lock`

## Security and Operational Notes

- Shell launching is enabled by Tauri allowlist and used for commands/scripts.
- File-system scope includes user home and appdata paths in current config.
- Input automation sends synthetic key events; treat macros as privileged local automation.

## Development Notes

Frontend stack:

- Svelte 4
- Vite 5
- `@tauri-apps/api` 1.6

Backend stack:

- Rust + Tauri v1
- `notify`, `open`, `serde`, `fs2`
- `tauri-plugin-autostart` (v1 branch)

Important source files:

- `src/App.svelte`: main UI, node editor, macro recorder, launcher
- `src-tauri/src/main.rs`: commands, tray/hotkeys, macro execution, persistence
- `src-tauri/tauri.conf.json`: window definitions and allowlist

If you are changing runtime behavior, keep frontend action names and backend macro action handlers in sync.
