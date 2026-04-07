#!/usr/bin/env bash

# If invoked as `sh script.sh`, restart under bash so bash options/features work.
if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -euo pipefail

cd "$(dirname "$0")/.."

mode="${1:-build}"

if [[ "$mode" != "build" && "$mode" != "dev" ]]; then
  echo "[FinNode] Unknown mode: $mode"
  echo "[FinNode] Use: build (default) or dev"
  exit 1
fi

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

if [[ "${OSTYPE:-}" == linux* ]]; then
  if ! command -v pkg-config >/dev/null 2>&1; then
    install_apt_packages pkg-config
  fi

  if ! pkg-config --exists gdk-3.0 || ! pkg-config --exists libsoup-2.4; then
    install_apt_packages libgtk-3-dev libayatana-appindicator3-dev librsvg2-dev libsoup2.4-dev patchelf
  fi

  if ! pkg-config --exists webkit2gtk-4.0 javascriptcoregtk-4.0 && ! pkg-config --exists webkit2gtk-4.1 javascriptcoregtk-4.1; then
    webkit_pkg=""

    if command -v apt-cache >/dev/null 2>&1 && apt-cache show libwebkit2gtk-4.0-dev >/dev/null 2>&1; then
      webkit_pkg="libwebkit2gtk-4.0-dev"
    elif command -v apt-cache >/dev/null 2>&1 && apt-cache show libwebkit2gtk-4.1-dev >/dev/null 2>&1; then
      webkit_pkg="libwebkit2gtk-4.1-dev"
    fi

    if [ -n "$webkit_pkg" ]; then
      install_apt_packages "$webkit_pkg"
    else
      cat <<'EOF'
[FinNode] Missing WebKitGTK development files and no supported apt package was detected.
Install one of these packages manually, then re-run the build:
  libwebkit2gtk-4.0-dev
  libwebkit2gtk-4.1-dev
EOF
      exit 1
    fi
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
    echo "[FinNode] WebKitGTK pkg-config files were not found after automatic install."
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

if [[ "$mode" == "dev" ]]; then
  echo "[FinNode] Starting desktop app in dev mode..."
  npx tauri dev
  exit 0
fi

echo "[FinNode] Building executable bundle..."
npx tauri build

echo "[FinNode] Done. Artifacts are under: src-tauri/target/release/bundle"
