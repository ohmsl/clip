#!/usr/bin/env bash
set -euo pipefail

# Installs system dependencies (GStreamer + Tauri Linux prerequisites)
# and workspace dependencies for the clip monorepo.

if ! command -v apt-get >/dev/null 2>&1; then
    echo "error: this script requires apt (Debian/Ubuntu)" >&2
    exit 1
fi

if ! command -v pnpm >/dev/null 2>&1; then
    echo "error: pnpm not found (https://pnpm.io/installation)" >&2
    exit 1
fi

if ! command -v cargo >/dev/null 2>&1; then
    echo "error: cargo not found (https://rustup.rs)" >&2
    exit 1
fi

SYSTEM_PACKAGES=(
    pkg-config
    build-essential
    curl
    wget
    file
    libgstreamer1.0-dev
    libgstreamer-plugins-base1.0-dev
    libgstreamer-plugins-bad1.0-dev
    gstreamer1.0-plugins-good
    gstreamer1.0-plugins-bad
    libwebkit2gtk-4.1-dev
    libxdo-dev
    libssl-dev
    libayatana-appindicator3-dev
    librsvg2-dev
)

SUDO=""
if [[ $EUID -ne 0 ]]; then
    SUDO="sudo"
fi

echo "==> Installing system dependencies..."
$SUDO apt-get update
$SUDO apt-get install -y "${SYSTEM_PACKAGES[@]}"

echo "==> Verifying GStreamer development libraries..."
if ! pkg-config --exists gstreamer-1.0 gstreamer-app-1.0 gstreamer-base-1.0; then
    echo "error: gstreamer dev libraries still missing after install" >&2
    exit 1
fi

echo "==> Installing workspace dependencies..."
pnpm install

for manifest in apps/core/Cargo.toml apps/ui/src-tauri/Cargo.toml; do
    echo "==> Fetching Rust dependencies ($manifest)..."
    cargo fetch --locked --manifest-path "$manifest"
done

echo "Setup complete. Run 'pnpm dev' to start."
