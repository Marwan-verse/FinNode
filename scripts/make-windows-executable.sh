#!/usr/bin/env bash

# If invoked as `sh script.sh`, restart under bash so bash options/features work.
if [ -z "${BASH_VERSION:-}" ]; then
  exec bash "$0" "$@"
fi

set -euo pipefail

cd "$(dirname "$0")/.."

resolve_windows_exe_basename() {
  local exe_name="${FINNODE_WINDOWS_EXE_NAME:-FinNode}"
  exe_name="${exe_name%.exe}"

  if [ -z "$exe_name" ]; then
    echo "FinNode"
    return
  fi

  echo "$exe_name"
}

resolve_windows_target_dir() {
  if [ -n "${FINNODE_WINDOWS_TARGET_DIR:-}" ]; then
    echo "${FINNODE_WINDOWS_TARGET_DIR}"
    return
  fi

  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    echo "${CARGO_TARGET_DIR}"
    return
  fi

  local repo_root
  repo_root="$(pwd)"

  # WSL/NTFS mounts can fail while creating tauri resource.lib; build in a Linux-local cache.
  if [[ "$repo_root" == /mnt/* ]]; then
    echo "$HOME/.cache/finnode/windows-target"
    return
  fi

  echo "$repo_root/src-tauri/target"
}

resolve_windows_target_triple() {
  local requested_target="${FINNODE_WINDOWS_TARGET_TRIPLE:-x86_64-pc-windows-msvc}"

  # Standalone mode defaults to MSVC because GNU builds require WebView2Loader.dll at runtime.
  if [[ "$requested_target" == *-windows-gnu ]] && [ "${FINNODE_ALLOW_WEBVIEW2_DLL:-0}" != "1" ]; then
    cat >&2 <<EOF
[FinNode] Refusing GNU target in standalone mode: $requested_target
[FinNode] GNU builds require WebView2Loader.dll next to finnode.exe.
[FinNode] Use MSVC for a single-file EXE:
  FINNODE_WINDOWS_TARGET_TRIPLE=x86_64-pc-windows-msvc npm run make:windows

[FinNode] If you intentionally want GNU + sidecar DLL, opt in explicitly:
  FINNODE_ALLOW_WEBVIEW2_DLL=1 FINNODE_WINDOWS_TARGET_TRIPLE=$requested_target npm run make:windows
EOF
    exit 1
  fi

  echo "$requested_target"
}

exe_imports_webview2_loader() {
  local exe_path="$1"

  if command -v llvm-objdump >/dev/null 2>&1; then
    if llvm-objdump -p "$exe_path" 2>/dev/null | grep -qi 'DLL Name: WebView2Loader.dll'; then
      return 0
    fi
    return 1
  fi

  if command -v objdump >/dev/null 2>&1; then
    if objdump -x "$exe_path" 2>/dev/null | grep -qi 'DLL Name: WebView2Loader.dll'; then
      return 0
    fi
    return 1
  fi

  # If import inspection tools are unavailable, do not block the build.
  return 1
}

ensure_writable_dir() {
  local dir="$1"
  mkdir -p "$dir"

  if [ ! -w "$dir" ]; then
    cat <<EOF
[FinNode] Target directory is not writable: $dir
[FinNode] Set a writable directory and retry, for example:
  FINNODE_WINDOWS_TARGET_DIR="$HOME/.cache/finnode/windows-target" bash ./scripts/make-windows-executable.sh
EOF
    exit 1
  fi
}

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

ensure_cargo_xwin() {
  if command -v cargo-xwin >/dev/null 2>&1; then
    return
  fi

  echo "[FinNode] Installing cargo-xwin..."
  cargo install cargo-xwin --locked
}

ensure_msvc_cross_toolchain_linux() {
  local missing=()

  if ! command -v clang-cl >/dev/null 2>&1; then
    missing+=(clang)
  fi

  if ! command -v lld-link >/dev/null 2>&1; then
    missing+=(lld)
  fi

  if [ "${#missing[@]}" -gt 0 ]; then
    install_apt_packages "${missing[@]}"
  fi

  ensure_cargo_xwin
}

ensure_windows_icon_assets() {
  local png_path="src-tauri/icons/icon.png"
  local ico_path="src-tauri/icons/icon.ico"

  if [ -f "$png_path" ] && command -v convert >/dev/null 2>&1; then
    echo "[FinNode] Syncing Windows icon from icon.png..."
    convert "$png_path" -define icon:auto-resize=16,24,32,48,64,128,256 "$ico_path"
  fi

  if [ ! -f "$ico_path" ]; then
    cat <<'EOF'
[FinNode] Missing Windows icon file: src-tauri/icons/icon.ico
[FinNode] Add icon.ico, or install ImageMagick so the script can generate it from icon.png:
  sudo apt-get update
  sudo apt-get install -y imagemagick
EOF
    exit 1
  fi
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

WINDOWS_TARGET_TRIPLE="$(resolve_windows_target_triple)"

if ! rustup target list --installed | grep -qx "$WINDOWS_TARGET_TRIPLE"; then
  echo "[FinNode] Installing Windows Rust target ($WINDOWS_TARGET_TRIPLE)..."
  rustup target add "$WINDOWS_TARGET_TRIPLE"
fi

case "$WINDOWS_TARGET_TRIPLE" in
  *-windows-gnu)
    if [ "$WINDOWS_TARGET_TRIPLE" != "x86_64-pc-windows-gnu" ]; then
      echo "[FinNode] Unsupported GNU target triple: $WINDOWS_TARGET_TRIPLE"
      echo "[FinNode] Supported GNU triple: x86_64-pc-windows-gnu"
      exit 1
    fi

    if ! command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
      install_apt_packages gcc-mingw-w64-x86-64
    fi
    ;;
  *-windows-msvc)
    if [[ "${OSTYPE:-}" == linux* ]]; then
      ensure_msvc_cross_toolchain_linux
    fi
    ;;
  *)
    echo "[FinNode] Unsupported windows target triple: $WINDOWS_TARGET_TRIPLE"
    echo "[FinNode] Use a Windows GNU/MSVC triple, for example:"
    echo "  FINNODE_WINDOWS_TARGET_TRIPLE=x86_64-pc-windows-msvc"
    exit 1
    ;;
esac

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

ensure_windows_icon_assets

echo "[FinNode] Web assets built. Starting Rust Windows compile (this can take several minutes on first run)..."

WINDOWS_TARGET_DIR="$(resolve_windows_target_dir)"
ensure_writable_dir "$WINDOWS_TARGET_DIR"

if [ "$WINDOWS_TARGET_DIR" != "$(pwd)/src-tauri/target" ]; then
  echo "[FinNode] Using CARGO_TARGET_DIR=$WINDOWS_TARGET_DIR"
fi

echo "[FinNode] Building Windows executable for $WINDOWS_TARGET_TRIPLE..."
if [[ "$WINDOWS_TARGET_TRIPLE" == *-windows-gnu ]]; then
  CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
    CARGO_TARGET_DIR="$WINDOWS_TARGET_DIR" \
    cargo build --release --target "$WINDOWS_TARGET_TRIPLE" --manifest-path src-tauri/Cargo.toml
elif [[ "$WINDOWS_TARGET_TRIPLE" == *-windows-msvc && "${OSTYPE:-}" == linux* ]]; then
  CARGO_TARGET_DIR="$WINDOWS_TARGET_DIR" \
    cargo xwin build --release --target "$WINDOWS_TARGET_TRIPLE" --manifest-path src-tauri/Cargo.toml
else
  CARGO_TARGET_DIR="$WINDOWS_TARGET_DIR" \
    cargo build --release --target "$WINDOWS_TARGET_TRIPLE" --manifest-path src-tauri/Cargo.toml
fi

WINDOWS_OUTPUT_DIR="$WINDOWS_TARGET_DIR/$WINDOWS_TARGET_TRIPLE/release"
WINDOWS_EXE_PATH="$WINDOWS_OUTPUT_DIR/finnode.exe"
WINDOWS_DLL_PATH="$WINDOWS_OUTPUT_DIR/WebView2Loader.dll"

if [ ! -f "$WINDOWS_EXE_PATH" ]; then
  echo "[FinNode] Build completed but expected executable was not found at: $WINDOWS_EXE_PATH"
  exit 1
fi

if [[ "$WINDOWS_TARGET_TRIPLE" == *-windows-msvc ]] && exe_imports_webview2_loader "$WINDOWS_EXE_PATH"; then
  cat <<EOF
[FinNode] Build output still imports WebView2Loader.dll:
  $WINDOWS_EXE_PATH
[FinNode] This is not a standalone executable.
[FinNode] Re-run with MSVC target and without GNU overrides.
EOF
  exit 1
fi

WINDOWS_EXPORT_DIR="${FINNODE_WINDOWS_EXPORT_DIR:-$(pwd)/artifacts/windows}"
WINDOWS_EXE_BASENAME="$(resolve_windows_exe_basename)"
WINDOWS_EXE_FILENAME="$WINDOWS_EXE_BASENAME.exe"
mkdir -p "$WINDOWS_EXPORT_DIR"

# Remove old exports first so Windows sees a fresh file write and icon metadata.
rm -f "$WINDOWS_EXPORT_DIR/$WINDOWS_EXE_FILENAME"
# Also clear historical default names if they differ from the chosen output name.
if [ "$WINDOWS_EXE_FILENAME" != "finnode.exe" ]; then
  rm -f "$WINDOWS_EXPORT_DIR/finnode.exe"
fi
if [ "$WINDOWS_EXE_FILENAME" != "FinNode.exe" ]; then
  rm -f "$WINDOWS_EXPORT_DIR/FinNode.exe"
fi

cp -f "$WINDOWS_EXE_PATH" "$WINDOWS_EXPORT_DIR/$WINDOWS_EXE_FILENAME"
if [[ "$WINDOWS_TARGET_TRIPLE" == *-windows-gnu ]] && [ -f "$WINDOWS_DLL_PATH" ]; then
  cp -f "$WINDOWS_DLL_PATH" "$WINDOWS_EXPORT_DIR/WebView2Loader.dll"
fi
if [[ "$WINDOWS_TARGET_TRIPLE" == *-windows-msvc ]]; then
  rm -f "$WINDOWS_EXPORT_DIR/WebView2Loader.dll"
fi

echo "[FinNode] Internal build output: $WINDOWS_EXE_PATH"
echo "[FinNode] Final exported artifacts: $WINDOWS_EXPORT_DIR"
echo "[FinNode] USE THIS FILE: $WINDOWS_EXPORT_DIR/$WINDOWS_EXE_FILENAME"
if [[ "$WINDOWS_TARGET_TRIPLE" == *-windows-msvc ]]; then
  echo "[FinNode] Target: $WINDOWS_TARGET_TRIPLE (WebView2 loader is statically linked)"
fi
if [[ "$WINDOWS_TARGET_TRIPLE" == *-windows-gnu ]]; then
  echo "[FinNode] Target: $WINDOWS_TARGET_TRIPLE"
  if [ -f "$WINDOWS_EXPORT_DIR/WebView2Loader.dll" ]; then
    echo "[FinNode] Runtime DLL: $WINDOWS_EXPORT_DIR/WebView2Loader.dll"
  else
    echo "[FinNode] Warning: WebView2Loader.dll was not found. GNU builds usually need it next to the EXE."
  fi
fi

if command -v wslpath >/dev/null 2>&1; then
  export_exe_windows_path="$(wslpath -w "$WINDOWS_EXPORT_DIR/$WINDOWS_EXE_FILENAME" 2>/dev/null || true)"
  if [ -n "$export_exe_windows_path" ]; then
    echo "[FinNode] Windows path: $export_exe_windows_path"
  fi
fi