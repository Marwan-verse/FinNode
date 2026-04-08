# FinNode Overlay

FinNode Overlay is a lightweight Rust + Tauri desktop overlay with a JavaScript/Svelte UI.

The app is designed for always-on desktop use:

- Transparent fullscreen overlay window.
- Glowing draggable nodes with animated links.
- Rust-native command and macro execution.
- Click-through mode for zero-interference background behavior.

## Architecture

### Frontend (Svelte)

- Renders nodes, links, and overlay controls.
- Handles drag interactions and local node persistence.
- Sends command and macro requests to Rust through Tauri invoke.

### Backend (Rust)

- Executes shell commands efficiently (`bash`, `powershell`, `cmd`, `zsh`).
- Runs macro step chains in a background thread.
- Toggles click-through with `set_ignore_cursor_events`.
- Exposes visibility and platform commands for UI state.

## Hotkeys

- `Alt+Shift+O`: Toggle click-through.
- `Alt+Shift+H`: Toggle overlay visibility.

## Local Development

1. Install dependencies:

```bash
npm install
```

2. Run frontend development server:

```bash
npm run dev
```

3. Run desktop shell:

```bash
npm run dev:desktop
```

## Build

- Web assets:

```bash
npm run build:web
```

- Desktop bundle:

```bash
npm run build:exe
```

- Windows bundle helper:

```bash
npm run make:windows
```