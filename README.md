# FinNode

FinNode is a Tauri HUD for desktop project nodes.

## What it does

- Transparent, borderless HUD shell with a frosted-glass layout.
- Rust backend for global shortcuts, layout persistence, and OS launches.
- Frontend canvas/svg node links with draggable project nodes.
- A `make` path that bundles the app through Tauri.

## Run locally

1. Install dependencies with `npm install`.
2. Start the web UI with `npm run dev`.
3. Start the desktop shell with `npm run dev:desktop`.

## Build

- Web assets: `npm run build:web`
- Desktop bundle: `npm run build:exe`
- Same bundle shortcut: `npm run make`
- One-command executable maker: `./scripts/make-executable.sh`
- Windows executable from Linux: `npm run make:windows`
- Windows executable helper: `./scripts/make-windows-executable.sh`

## Notes

- Layout data is saved to the app config directory as `FinNode/config.json`.
- Global shortcut is `Alt+S` by default.
- The Windows build output is `src-tauri/target/x86_64-pc-windows-gnu/release/finnode.exe` with `WebView2Loader.dll` beside it.