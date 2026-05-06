#!/bin/bash
# Downloads stremio-service release files and static ffmpeg for all platforms.
# Used by CI and for local development.
#
# Usage:
#   bash src-tauri/scripts/download-deps.sh --platform <platform>
#
# Platforms: windows, macos-arm64, macos-x86_64, linux
#
# Run from the project root.

set -euo pipefail

# --- Configuration ---
SERVICE_VERSION="v0.1.21"
SERVICE_REPO="Stremio/stremio-service"
TAURI_DIR="src-tauri"
TEMP_DIR=$(mktemp -d)

# Static FFmpeg / MPV sources
FFMPEG_WIN_URL="https://github.com/GyanD/codexffmpeg/releases/download/7.1.1/ffmpeg-7.1.1-essentials_build.zip"
FFMPEG_LINUX_URL="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"
FFMPEG_MAC_URL="https://evermeet.cx/ffmpeg/getrelease"
FFPROBE_MAC_URL="https://evermeet.cx/ffmpeg/getrelease/ffprobe"
MPV_DEV_WIN_URL="https://sourceforge.net/projects/mpv-player-windows/files/libmpv/mpv-dev-x86_64-v3-20240211-git-f5c4f0b.7z/download"

# --- Parse arguments ---
PLATFORM=""
while [[ $# -gt 0 ]]; do
    case "$1" in
        --platform)
            PLATFORM="$2"
            shift 2
            ;;
        *)
            echo "Unknown argument: $1" >&2
            exit 1
            ;;
    esac
done

if [[ -z "$PLATFORM" ]]; then
    echo "Usage: $0 --platform <windows|macos-arm64|macos-x86_64|linux>" >&2
    exit 1
fi

# --- Helper functions ---
cleanup() {
    rm -rf "$TEMP_DIR"
}
trap cleanup EXIT

ensure_dirs() {
    mkdir -p "$TAURI_DIR/binaries" "$TAURI_DIR/resources" "$TAURI_DIR/mpv-dev"
}

extract_7z() {
    local archive="$1"
    local destination="$2"

    if command -v 7z &>/dev/null; then
        7z x "$archive" -o"$destination" -y
    elif command -v 7zz &>/dev/null; then
        7zz x "$archive" -o"$destination" -y
    else
        echo "ERROR: 7z or 7zz not found. Install 7-Zip/p7zip." >&2
        exit 1
    fi
}

# --- Platform-specific downloads ---

download_windows() {
    local ASSET="stremio-service-windows.zip"
    local TARGET_TRIPLE="x86_64-pc-windows-msvc"

    echo "==> Downloading stremio-service $SERVICE_VERSION (Windows)..."
    gh release download "$SERVICE_VERSION" --repo "$SERVICE_REPO" --pattern "$ASSET" --dir "$TEMP_DIR"
    unzip -o "$TEMP_DIR/$ASSET" -d "$TEMP_DIR/extracted"

    echo "==> Downloading static FFmpeg (Windows)..."
    curl -L "$FFMPEG_WIN_URL" -o "$TEMP_DIR/ffmpeg.zip"
    unzip -o "$TEMP_DIR/ffmpeg.zip" -d "$TEMP_DIR/ffmpeg"

    # Find ffmpeg.exe inside the extracted archive
    FFMPEG_EXE=$(find "$TEMP_DIR/ffmpeg" -type f -name "ffmpeg.exe" | head -n 1)
    FFPROBE_EXE=$(find "$TEMP_DIR/ffmpeg" -type f -name "ffprobe.exe" | head -n 1)

    if [[ -z "$FFMPEG_EXE" || -z "$FFPROBE_EXE" ]]; then
        echo "ERROR: Could not find ffmpeg.exe or ffprobe.exe in downloaded archive" >&2
        exit 1
    fi

    echo "==> Downloading libmpv development files (Windows)..."
    curl -L "$MPV_DEV_WIN_URL" -o "$TEMP_DIR/mpv-dev.7z"
    extract_7z "$TEMP_DIR/mpv-dev.7z" "$TEMP_DIR/mpv-dev"

    LIBMPV_DLL=$(find "$TEMP_DIR/mpv-dev" -type f -name "libmpv-2.dll" | head -n 1)
    MPV_LIB=$(find "$TEMP_DIR/mpv-dev" -type f -name "mpv.lib" | head -n 1)
    MPV_DEF=$(find "$TEMP_DIR/mpv-dev" -type f -name "mpv.def" | head -n 1)

    if [[ -z "$LIBMPV_DLL" ]]; then
        echo "ERROR: Could not find libmpv-2.dll in downloaded MPV archive" >&2
        exit 1
    fi

    if [[ -z "$MPV_LIB" && -n "$MPV_DEF" ]]; then
        echo "==> mpv.lib not found; attempting to generate it from mpv.def..."
        cp "$MPV_DEF" "$TAURI_DIR/mpv-dev/mpv.def"
        if command -v lib.exe &>/dev/null; then
            (cd "$TAURI_DIR/mpv-dev" && lib.exe /def:mpv.def /name:libmpv-2.dll /out:mpv.lib /MACHINE:X64)
            MPV_LIB="$TAURI_DIR/mpv-dev/mpv.lib"
        elif command -v llvm-lib &>/dev/null; then
            (cd "$TAURI_DIR/mpv-dev" && llvm-lib /def:mpv.def /name:libmpv-2.dll /out:mpv.lib /MACHINE:X64)
            MPV_LIB="$TAURI_DIR/mpv-dev/mpv.lib"
        fi
    fi

    if [[ -z "$MPV_LIB" || ! -f "$MPV_LIB" ]]; then
        echo "ERROR: Could not find or generate mpv.lib for MSVC linking" >&2
        echo "       Install Visual Studio Build Tools, open a Developer shell, then re-run:" >&2
        echo "       npm run setup -- --force" >&2
        exit 1
    fi

    # Place files
    cp "$TEMP_DIR/extracted/stremio-runtime.exe" "$TAURI_DIR/binaries/stremio-runtime-${TARGET_TRIPLE}.exe"
    cp "$TEMP_DIR/extracted/server.js" "$TAURI_DIR/resources/server.cjs"
    cp "$FFMPEG_EXE" "$TAURI_DIR/resources/ffmpeg.exe"
    cp "$FFPROBE_EXE" "$TAURI_DIR/resources/ffprobe.exe"
    cp "$LIBMPV_DLL" "$TAURI_DIR/resources/libmpv-2.dll"
    cp "$MPV_LIB" "$TAURI_DIR/mpv-dev/mpv.lib"
    [[ -n "$MPV_DEF" ]] && cp "$MPV_DEF" "$TAURI_DIR/mpv-dev/mpv.def"

    echo "==> Windows dependencies ready:"
    echo "    $TAURI_DIR/binaries/stremio-runtime-${TARGET_TRIPLE}.exe"
    echo "    $TAURI_DIR/resources/server.cjs"
    echo "    $TAURI_DIR/resources/ffmpeg.exe"
    echo "    $TAURI_DIR/resources/ffprobe.exe"
    echo "    $TAURI_DIR/resources/libmpv-2.dll"
    echo "    $TAURI_DIR/mpv-dev/mpv.lib"
}

download_macos() {
    local ARCH="$1"  # arm64 or x86_64
    local ASSET="stremio-service-macos.zip"

    if [[ "$ARCH" == "arm64" ]]; then
        local TARGET_TRIPLE="aarch64-apple-darwin"
    else
        local TARGET_TRIPLE="x86_64-apple-darwin"
    fi

    echo "==> Downloading stremio-service $SERVICE_VERSION (macOS)..."
    gh release download "$SERVICE_VERSION" --repo "$SERVICE_REPO" --pattern "$ASSET" --dir "$TEMP_DIR"
    unzip -o "$TEMP_DIR/$ASSET" -d "$TEMP_DIR/extracted"

    echo "==> Downloading static FFmpeg (macOS)..."
    curl -L "$FFMPEG_MAC_URL" -o "$TEMP_DIR/ffmpeg.7z"
    curl -L "$FFPROBE_MAC_URL" -o "$TEMP_DIR/ffprobe.7z"

    # Extract ffmpeg and ffprobe from 7z archives
    extract_7z "$TEMP_DIR/ffmpeg.7z" "$TEMP_DIR/ffmpeg-extracted"
    extract_7z "$TEMP_DIR/ffprobe.7z" "$TEMP_DIR/ffprobe-extracted"

    FFMPEG_BIN=$(find "$TEMP_DIR/ffmpeg-extracted" -type f -name "ffmpeg" | head -n 1)
    FFPROBE_BIN=$(find "$TEMP_DIR/ffprobe-extracted" -type f -name "ffprobe" | head -n 1)

    if [[ -z "$FFMPEG_BIN" || -z "$FFPROBE_BIN" ]]; then
        echo "ERROR: Could not find ffmpeg or ffprobe in downloaded archives" >&2
        exit 1
    fi

    # The macOS stremio-service zip contains stremio-runtime (no .exe extension)
    local RUNTIME_BIN=""
    if [[ -f "$TEMP_DIR/extracted/stremio-runtime" ]]; then
        RUNTIME_BIN="$TEMP_DIR/extracted/stremio-runtime"
    elif [[ -f "$TEMP_DIR/extracted/stremio-service.app/Contents/MacOS/stremio-runtime" ]]; then
        RUNTIME_BIN="$TEMP_DIR/extracted/stremio-service.app/Contents/MacOS/stremio-runtime"
    else
        # Try to find it anywhere in the extracted directory
        RUNTIME_BIN=$(find "$TEMP_DIR/extracted" -type f -name "stremio-runtime" | head -n 1)
    fi

    if [[ -z "$RUNTIME_BIN" ]]; then
        echo "ERROR: Could not find stremio-runtime in stremio-service macOS archive" >&2
        echo "Contents of extracted directory:"
        find "$TEMP_DIR/extracted" -type f
        exit 1
    fi

    # Find server.js
    local SERVER_JS=""
    SERVER_JS=$(find "$TEMP_DIR/extracted" -type f -name "server.js" | head -n 1)
    if [[ -z "$SERVER_JS" ]]; then
        echo "ERROR: Could not find server.js in stremio-service macOS archive" >&2
        exit 1
    fi

    # Place files
    cp "$RUNTIME_BIN" "$TAURI_DIR/binaries/stremio-runtime-${TARGET_TRIPLE}"
    chmod +x "$TAURI_DIR/binaries/stremio-runtime-${TARGET_TRIPLE}"
    cp "$SERVER_JS" "$TAURI_DIR/resources/server.cjs"
    cp "$FFMPEG_BIN" "$TAURI_DIR/resources/ffmpeg"
    cp "$FFPROBE_BIN" "$TAURI_DIR/resources/ffprobe"
    chmod +x "$TAURI_DIR/resources/ffmpeg" "$TAURI_DIR/resources/ffprobe"

    echo "==> macOS ($ARCH) dependencies ready:"
    echo "    $TAURI_DIR/binaries/stremio-runtime-${TARGET_TRIPLE}"
    echo "    $TAURI_DIR/resources/server.cjs"
    echo "    $TAURI_DIR/resources/ffmpeg"
    echo "    $TAURI_DIR/resources/ffprobe"
}

download_linux() {
    local TARGET_TRIPLE="x86_64-unknown-linux-gnu"
    local DEB_ASSET="stremio-service_amd64.deb"

    echo "==> Downloading stremio-service $SERVICE_VERSION (Linux)..."
    gh release download "$SERVICE_VERSION" --repo "$SERVICE_REPO" --pattern "$DEB_ASSET" --dir "$TEMP_DIR"

    # Extract files from the .deb package
    mkdir -p "$TEMP_DIR/deb-extracted"
    dpkg-deb -x "$TEMP_DIR/$DEB_ASSET" "$TEMP_DIR/deb-extracted"

    # Find stremio-runtime and server.js in the deb contents
    local RUNTIME_BIN=""
    RUNTIME_BIN=$(find "$TEMP_DIR/deb-extracted" -type f -name "stremio-runtime" | head -n 1)
    local SERVER_JS=""
    SERVER_JS=$(find "$TEMP_DIR/deb-extracted" -type f -name "server.js" | head -n 1)

    if [[ -z "$RUNTIME_BIN" ]]; then
        echo "ERROR: Could not find stremio-runtime in stremio-service deb package" >&2
        echo "Contents of deb:"
        find "$TEMP_DIR/deb-extracted" -type f
        exit 1
    fi
    if [[ -z "$SERVER_JS" ]]; then
        echo "ERROR: Could not find server.js in stremio-service deb package" >&2
        exit 1
    fi

    echo "==> Downloading static FFmpeg (Linux)..."
    curl -L "$FFMPEG_LINUX_URL" -o "$TEMP_DIR/ffmpeg-linux.tar.xz"
    mkdir -p "$TEMP_DIR/ffmpeg-linux"
    tar -xf "$TEMP_DIR/ffmpeg-linux.tar.xz" -C "$TEMP_DIR/ffmpeg-linux" --strip-components=1

    FFMPEG_BIN="$TEMP_DIR/ffmpeg-linux/ffmpeg"
    FFPROBE_BIN="$TEMP_DIR/ffmpeg-linux/ffprobe"

    if [[ ! -f "$FFMPEG_BIN" || ! -f "$FFPROBE_BIN" ]]; then
        echo "ERROR: Could not find ffmpeg or ffprobe in downloaded archive" >&2
        exit 1
    fi

    # Place files
    cp "$RUNTIME_BIN" "$TAURI_DIR/binaries/stremio-runtime-${TARGET_TRIPLE}"
    chmod +x "$TAURI_DIR/binaries/stremio-runtime-${TARGET_TRIPLE}"
    cp "$SERVER_JS" "$TAURI_DIR/resources/server.cjs"
    cp "$FFMPEG_BIN" "$TAURI_DIR/resources/ffmpeg"
    cp "$FFPROBE_BIN" "$TAURI_DIR/resources/ffprobe"
    chmod +x "$TAURI_DIR/resources/ffmpeg" "$TAURI_DIR/resources/ffprobe"

    echo "==> Linux dependencies ready:"
    echo "    $TAURI_DIR/binaries/stremio-runtime-${TARGET_TRIPLE}"
    echo "    $TAURI_DIR/resources/server.cjs"
    echo "    $TAURI_DIR/resources/ffmpeg"
    echo "    $TAURI_DIR/resources/ffprobe"
}

# --- Main ---
ensure_dirs

case "$PLATFORM" in
    windows)
        download_windows
        ;;
    macos-arm64)
        download_macos "arm64"
        ;;
    macos-x86_64)
        download_macos "x86_64"
        ;;
    linux)
        download_linux
        ;;
    *)
        echo "ERROR: Unknown platform '$PLATFORM'" >&2
        echo "Supported platforms: windows, macos-arm64, macos-x86_64, linux" >&2
        exit 1
        ;;
esac

echo "==> All dependencies downloaded successfully."
