#!/usr/bin/env bash

# If invoked as `sh script.sh`, restart under bash so bash options/features work.
if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -euo pipefail

cd "$(dirname "$0")/.."

if ! command -v cargo >/dev/null 2>&1; then
  echo "[FinNode] Rust toolchain not found. Installing rustup locally..."
  curl https://sh.rustup.rs -sSf | sh -s -- -y
  # shellcheck disable=SC1091
  source "$HOME/.cargo/env"
fi

if [[ "${OSTYPE:-}" == linux* ]]; then
  if ! command -v pkg-config >/dev/null 2>&1 || ! pkg-config --exists gdk-3.0 webkit2gtk-4.0; then
    cat <<'EOF'
[FinNode] Missing Linux system dependencies for Tauri.
Install them, then re-run this script:
  sudo apt-get update
  sudo apt-get install -y \
    pkg-config \
    libwebkit2gtk-4.0-dev \
    libgtk-3-dev \
    libayatana-appindicator3-dev \
    librsvg2-dev \
    patchelf
EOF
    exit 1
  fi
fi

echo "[FinNode] Installing npm dependencies..."
npm install

echo "[FinNode] Building executable bundle..."
npm run build:exe

echo "[FinNode] Done. Artifacts are under: src-tauri/target/release/bundle"
