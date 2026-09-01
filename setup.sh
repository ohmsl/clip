#!/usr/bin/env bash
set -euo pipefail

# Installs system dependencies (GStreamer and native GPUI prerequisites) and workspace
# dependencies for the clip monorepo.
#
# Supported platforms:
#   - Debian/Ubuntu Linux (apt): installs everything, including GStreamer.
#   - Windows (run from Git Bash / MSYS2): detects an existing GStreamer
#     install; if missing, prints instructions with a download link.
#
# Windows GStreamer download (MSVC x86_64):
#   https://gstreamer.freedesktop.org/download/

GSTREAMER_VERSION="1.28.6"
GSTREAMER_WIN_URL="https://gstreamer.freedesktop.org/data/pkg/windows/${GSTREAMER_VERSION}/msvc/gstreamer-1.0-msvc-x86_64-${GSTREAMER_VERSION}.exe"

error() {
    echo "error: $*" >&2
    exit 1
}

require_cmd() {
    local cmd="$1"
    local url="${2:-}"
    if ! command -v "$cmd" >/dev/null 2>&1; then
        error "$cmd not found${url:+ ($url)}"
    fi
}

case "$(uname -s)" in
    Linux*)
        PLATFORM="linux"
        ;;
    MINGW*|MSYS*|CYGWIN*)
        PLATFORM="windows"
        ;;
    *)
        error "unsupported platform: $(uname -s)"
        ;;
esac

require_cmd cargo "https://rustup.rs"

# ---------------------------------------------------------------------------
# Linux (Debian/Ubuntu)
# ---------------------------------------------------------------------------

setup_linux() {
    require_cmd apt-get

    local SYSTEM_PACKAGES=(
        pkg-config
        build-essential
        clang
        cmake
        curl
        wget
        file
        git
        jq
        libasound2-dev
        libfontconfig-dev
        libgit2-dev
        libglib2.0-dev
        libssl-dev
        libva-dev
        libvulkan1
        libwayland-dev
        libx11-xcb-dev
        libxkbcommon-x11-dev
        libzstd-dev
        pipewire
        xdg-desktop-portal
        libgstreamer1.0-dev
        libgstreamer-plugins-base1.0-dev
        libgstreamer-plugins-bad1.0-dev
        gstreamer1.0-plugins-good
        gstreamer1.0-plugins-bad
    )

    local SUDO=""
    if [[ $EUID -ne 0 ]]; then
        SUDO="sudo"
    fi

    echo "==> Installing system dependencies..."
    $SUDO apt-get update
    $SUDO apt-get install -y "${SYSTEM_PACKAGES[@]}"

    echo "==> Verifying GStreamer development libraries..."
    if ! pkg-config --exists gstreamer-1.0 gstreamer-app-1.0 gstreamer-base-1.0; then
        error "gstreamer dev libraries still missing after install"
    fi
}

# ---------------------------------------------------------------------------
# Windows
# ---------------------------------------------------------------------------

to_unix_path() {
    if command -v cygpath >/dev/null 2>&1; then
        cygpath -u "$1"
    else
        printf '%s' "$1"
    fi
}

to_win_path() {
    if command -v cygpath >/dev/null 2>&1; then
        cygpath -w "$1"
    else
        printf '%s' "$1"
    fi
}

# Reads a REG_SZ / REG_EXPAND_SZ value from the registry. Uses //v so MSYS
# does not path-mangle the switch.
reg_get_value() {
    local hive="$1"
    local name="$2"
    local value=""
    value=$(reg query "$hive" //v "$name" 2>/dev/null | tr -d '\r' | sed -n 's/^.*REG_[A-Z_]*SZ[[:space:]]*//p') || true
    [[ -n "$value" ]] || return 1
    printf '%s' "$value"
}

is_gstreamer_root() {
    [[ -n "$1" && -f "$1/bin/gstreamer-1.0-0.dll" && -f "$1/lib/pkgconfig/gstreamer-1.0.pc" ]]
}

# Prints the GStreamer MSVC x86_64 root as a unix-style path, if found.
find_gstreamer_root() {
    local candidate=""

    if [[ -n "${GSTREAMER_1_0_ROOT_MSVC_X86_64:-}" ]]; then
        candidate=$(to_unix_path "${GSTREAMER_1_0_ROOT_MSVC_X86_64}")
        if is_gstreamer_root "$candidate"; then
            printf '%s' "$candidate"
            return 0
        fi
    fi

    local hive
    for hive in \
        'HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment' \
        'HKCU\Environment'; do
        candidate=$(reg_get_value "$hive" GSTREAMER_1_0_ROOT_MSVC_X86_64 || true)
        if [[ -n "$candidate" ]]; then
            candidate=$(to_unix_path "$candidate")
            if is_gstreamer_root "$candidate"; then
                printf '%s' "$candidate"
                return 0
            fi
        fi
    done

    for candidate in \
        '/c/Program Files/gstreamer/1.0/msvc_x86_64' \
        "$(to_unix_path "${LOCALAPPDATA:-$HOME\\AppData\\Local}")/Programs/gstreamer/1.0/msvc_x86_64"; do
        if is_gstreamer_root "$candidate"; then
            printf '%s' "$candidate"
            return 0
        fi
    done

    return 1
}

winget_available() {
    command -v winget.exe >/dev/null 2>&1
}

ensure_persisted_env() {
    local gst_root_win="$1"   # no trailing backslash, e.g. C:\...\msvc_x86_64
    local gst_bin_win="${gst_root_win}\\bin"

    # The installer usually persists GSTREAMER_1_0_ROOT_MSVC_X86_64, but some
    # 1.28.x releases have a bug where it is not written. Fix it up if missing.
    if ! reg_get_value 'HKLM\SYSTEM\CurrentControlSet\Control\Session Manager\Environment' GSTREAMER_1_0_ROOT_MSVC_X86_64 >/dev/null \
        && ! reg_get_value 'HKCU\Environment' GSTREAMER_1_0_ROOT_MSVC_X86_64 >/dev/null; then
        echo "==> Persisting GSTREAMER_1_0_ROOT_MSVC_X86_64 for future shells..."
        powershell.exe -NoProfile -Command "[Environment]::SetEnvironmentVariable('GSTREAMER_1_0_ROOT_MSVC_X86_64', '$gst_root_win\\', 'User')"
    fi

    # Make sure <root>\bin is on the user PATH so the DLLs load at runtime.
    local user_path
    user_path=$(powershell.exe -NoProfile -Command "[Environment]::GetEnvironmentVariable('Path','User')" | tr -d '\r')
    if ! printf '%s' "$user_path" | grep -qiF "$gst_bin_win"; then
        echo "==> Adding GStreamer bin directory to your user PATH..."
        powershell.exe -NoProfile -Command "\$p=[Environment]::GetEnvironmentVariable('Path','User'); if (-not \$p) { \$p = '' }; [Environment]::SetEnvironmentVariable('Path', (\$p.TrimEnd(';') + ';$gst_bin_win'), 'User')"
        export PATH="$gst_root_win\\bin;$PATH"
        echo "note: restart your shell afterwards so the updated PATH is picked up." >&2
    fi
}

has_msvc_tools() {
    local vswhere='/c/Program Files (x86)/Microsoft Visual Studio/Installer/vswhere.exe'
    [[ -f "$vswhere" ]] || return 1
    "$vswhere" \
        -latest -products '*' \
        -requires Microsoft.VisualStudio.Component.VC.Tools.x86.x64 \
        -property installationPath 2>/dev/null | tr -d '\r' | grep -q .
}

ensure_msvc_tools() {
    echo "==> Checking for MSVC build tools..."
    if has_msvc_tools; then
        echo "    Found Visual Studio C++ build tools."
        return 0
    fi

    echo "warning: Visual Studio C++ build tools not found (required to compile Rust)." >&2
    echo "         Install them from https://visualstudio.microsoft.com/visual-cpp-build-tools/" >&2
    if winget_available; then
        echo "         or run: winget install -e --id Microsoft.VisualStudio.2022.BuildTools --override \"--quiet --wait --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended\"" >&2
    fi
}

print_gstreamer_instructions() {
    cat >&2 <<EOF

error: GStreamer (MSVC x86_64) is required but was not found on this machine.

Install it manually:
  1. Download the MSVC x86_64 installer (${GSTREAMER_VERSION}):
        ${GSTREAMER_WIN_URL}
     or pick a version yourself:
        https://gstreamer.freedesktop.org/download/
  2. Run the installer and keep the default installation type
     ("Runtime and development headers") - it includes the development
     files needed to build clip.
  3. Re-run this script.

EOF
}

setup_windows() {
    echo "==> Locating GStreamer (MSVC x86_64)..."
    local gst_root
    if ! gst_root=$(find_gstreamer_root); then
        print_gstreamer_instructions
        exit 1
    fi

    local gst_root_win
    gst_root_win=$(to_win_path "$gst_root")

    echo "    Found GStreamer at $gst_root_win"
    export GSTREAMER_1_0_ROOT_MSVC_X86_64="${gst_root_win}\\"
    export PATH="$gst_root/bin:$PATH"

    ensure_persisted_env "$gst_root_win"
    ensure_msvc_tools
}

# ---------------------------------------------------------------------------
# Workspace dependencies (shared)
# ---------------------------------------------------------------------------

if [[ "$PLATFORM" == "linux" ]]; then
    setup_linux
else
    setup_windows
fi

for manifest in apps/core/Cargo.toml apps/ui/Cargo.toml; do
    echo "==> Fetching Rust dependencies ($manifest)..."
    cargo fetch --manifest-path "$manifest"
done

echo "Setup complete. Run 'cargo run --manifest-path apps/ui/Cargo.toml' to start the UI."
