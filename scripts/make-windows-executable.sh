#!/usr/bin/env bash

# If invoked as `sh script.sh`, restart under bash so bash options/features work.
if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -euo pipefail

cd "$(dirname "$0")/.."

run_privileged() {
  if command -v sudo >/dev/null 2>&1; then
    sudo "$@"
    return
  fi

  if [ "$(id -u)" -eq 0 ]; then
    "$@"
    return
  fi

  cat <<'EOF'
[FinNode] Need elevated privileges to install system dependencies.
Install sudo or run this script as root.
EOF
  exit 1
}

install_apt_packages() {
  local packages=("$@")

  if ! command -v apt-get >/dev/null 2>&1; then
    echo "[FinNode] apt-get is not available. Please install these packages manually: ${packages[*]}"
    exit 1
  fi

  echo "[FinNode] Installing missing system dependencies: ${packages[*]}"
  if [ "${APT_UPDATED:-0}" -eq 0 ]; then
    run_privileged apt-get update
    APT_UPDATED=1
  fi
  run_privileged apt-get install -y "${packages[@]}"
}

# Load Cargo path first when rustup is already installed.
if [ -s "$HOME/.cargo/env" ]; then
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

if ! command -v cargo >/dev/null 2>&1; then
  echo "[FinNode] Rust toolchain not found. Installing rustup locally..."
  curl https://sh.rustup.rs -sSf | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

if ! rustup target list --installed | grep -qx 'x86_64-pc-windows-gnu'; then
  echo "[FinNode] Installing Windows Rust target..."
  rustup target add x86_64-pc-windows-gnu
fi

if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
  install_apt_packages gcc-mingw-w64-x86-64
fi

if ! command -v npm >/dev/null 2>&1 && [ -s "$HOME/.nvm/nvm.sh" ]; then
  # shellcheck disable=SC1091
  source "$HOME/.nvm/nvm.sh"
fi

if ! command -v npm >/dev/null 2>&1; then
  cat <<'EOF'
[FinNode] npm is not installed.
Install Node.js/npm, then re-run this script.
If you use nvm:
  source "$HOME/.nvm/nvm.sh"
  nvm install --lts
EOF
  exit 1
fi

echo "[FinNode] Installing npm dependencies..."
npm install --include=dev

echo "[FinNode] Building web assets..."
npm run build:web

if [ -f src-tauri/icons/icon.png ] && command -v convert >/dev/null 2>&1; then
  echo "[FinNode] Syncing Windows icon from icon.png..."
  convert src-tauri/icons/icon.png -define icon:auto-resize=16,24,32,48,64,128,256 src-tauri/icons/icon.ico
fi

echo "[FinNode] Web assets built. Starting Rust Windows compile (this can take several minutes on first run)..."

echo "[FinNode] Building Windows executable..."
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
  cargo build --release --target x86_64-pc-windows-gnu --manifest-path src-tauri/Cargo.toml

echo "[FinNode] Done. Windows executable is at: src-tauri/target/x86_64-pc-windows-gnu/release/finnode.exe"
echo "[FinNode] Supporting runtime file: src-tauri/target/x86_64-pc-windows-gnu/release/WebView2Loader.dll"
