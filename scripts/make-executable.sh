#!/usr/bin/env bash

# If invoked as `sh script.sh`, restart under bash so bash options/features work.
if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -euo pipefail

cd "$(dirname "$0")/.."

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

if [[ "${OSTYPE:-}" == linux* ]]; then
  if ! command -v pkg-config >/dev/null 2>&1 || ! pkg-config --exists gdk-3.0 libsoup-2.4; then
    cat <<'EOF'
[FinNode] Missing Linux system dependencies for Tauri.
Install them, then re-run this script:
  sudo apt-get update
  sudo apt-get install -y \
    pkg-config \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    libsoup2.4-dev \
    patchelf

Then install one WebKitGTK dev package based on your distro:
  Ubuntu 20.04/22.04: libwebkit2gtk-4.0-dev
  Ubuntu 24.04+:      libwebkit2gtk-4.1-dev
EOF
    exit 1
  fi

  has_webkit40=false
  has_webkit41=false

  if pkg-config --exists webkit2gtk-4.0 javascriptcoregtk-4.0; then
    has_webkit40=true
  fi

  if pkg-config --exists webkit2gtk-4.1 javascriptcoregtk-4.1; then
    has_webkit41=true
  fi

  if [[ "$has_webkit40" == false && "$has_webkit41" == false ]]; then
    cat <<'EOF'
[FinNode] Missing WebKitGTK development files required by Tauri.
Install one of:
  Ubuntu 20.04/22.04: libwebkit2gtk-4.0-dev
  Ubuntu 24.04+:      libwebkit2gtk-4.1-dev
EOF
    exit 1
  fi

  if [[ "$has_webkit40" == false && "$has_webkit41" == true ]]; then
    compat_root="$PWD/src-tauri/target/linux-webkit-compat"
    compat_pkg="$compat_root/pkgconfig"
    compat_lib="$compat_root/lib"

    mkdir -p "$compat_pkg" "$compat_lib"

    webkit_pc_dir="$(pkg-config --variable pcfiledir webkit2gtk-4.1)"
    webkit_lib_dir="$(pkg-config --variable libdir webkit2gtk-4.1)"
    jscore_pc_dir="$(pkg-config --variable pcfiledir javascriptcoregtk-4.1)"
    jscore_lib_dir="$(pkg-config --variable libdir javascriptcoregtk-4.1)"

    ln -sf "$webkit_pc_dir/webkit2gtk-4.1.pc" "$compat_pkg/webkit2gtk-4.0.pc"
    ln -sf "$webkit_pc_dir/webkit2gtk-web-extension-4.1.pc" "$compat_pkg/webkit2gtk-web-extension-4.0.pc"
    ln -sf "$jscore_pc_dir/javascriptcoregtk-4.1.pc" "$compat_pkg/javascriptcoregtk-4.0.pc"

    ln -sf "$webkit_lib_dir/libwebkit2gtk-4.1.so" "$compat_lib/libwebkit2gtk-4.0.so"
    ln -sf "$jscore_lib_dir/libjavascriptcoregtk-4.1.so" "$compat_lib/libjavascriptcoregtk-4.0.so"

    export PKG_CONFIG_PATH="$compat_pkg${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
    export LIBRARY_PATH="$compat_lib${LIBRARY_PATH:+:$LIBRARY_PATH}"
    export LD_LIBRARY_PATH="$compat_lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"

    echo "[FinNode] Using WebKitGTK 4.1 compatibility shim for Tauri v1."
  fi
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

echo "[FinNode] Building executable bundle..."
npx tauri build

echo "[FinNode] Done. Artifacts are under: src-tauri/target/release/bundle"
