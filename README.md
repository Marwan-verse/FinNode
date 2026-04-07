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

### Linux desktop prerequisites

`npm run build:exe` now attempts to install missing Linux dependencies automatically on apt-based systems.

If you want to install them manually (or if your distro is not apt-based), use:

```bash
sudo apt-get update
sudo apt-get install -y \
	pkg-config \
	libgtk-3-dev \
	libayatana-appindicator3-dev \
	librsvg2-dev \
	libsoup2.4-dev \
	patchelf

# Ubuntu 20.04/22.04
sudo apt-get install -y libwebkit2gtk-4.0-dev

# Ubuntu 24.04+
sudo apt-get install -y libwebkit2gtk-4.1-dev
```

`npm run build:windows` also auto-installs `gcc-mingw-w64-x86-64` on apt-based Linux when missing.

## Notes

- Layout data is saved to the app config directory as `FinNode/config.json`.
- Global shortcut is `Alt+S` by default.
- The Windows build output is `src-tauri/target/x86_64-pc-windows-gnu/release/finnode.exe` with `WebView2Loader.dll` beside it.