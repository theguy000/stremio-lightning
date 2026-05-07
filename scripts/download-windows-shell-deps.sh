#!/bin/bash
# Downloads runtime files for the direct Windows WebView2 shell.
# Run from the project root:
#   bash scripts/download-windows-shell-deps.sh

set -euo pipefail

SERVICE_VERSION="v0.1.21"
SERVICE_REPO="Stremio/stremio-service"
WINDOWS_DIR="crates/stremio-lightning-windows"
TEMP_DIR=$(mktemp -d)

FFMPEG_WIN_URL="https://github.com/GyanD/codexffmpeg/releases/download/7.1.1/ffmpeg-7.1.1-essentials_build.zip"
MPV_DEV_WIN_URL="https://sourceforge.net/projects/mpv-player-windows/files/libmpv/mpv-dev-x86_64-v3-20240211-git-f5c4f0b.7z/download"

cleanup() {
    rm -rf "$TEMP_DIR"
}
trap cleanup EXIT

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

mkdir -p "$WINDOWS_DIR/resources" "$WINDOWS_DIR/mpv-dev"

echo "==> Downloading stremio-service $SERVICE_VERSION (Windows)..."
gh release download "$SERVICE_VERSION" --repo "$SERVICE_REPO" --pattern "stremio-service-windows.zip" --dir "$TEMP_DIR"
unzip -o "$TEMP_DIR/stremio-service-windows.zip" -d "$TEMP_DIR/service"

echo "==> Downloading static FFmpeg (Windows)..."
curl -L "$FFMPEG_WIN_URL" -o "$TEMP_DIR/ffmpeg.zip"
unzip -o "$TEMP_DIR/ffmpeg.zip" -d "$TEMP_DIR/ffmpeg"

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
    cp "$MPV_DEF" "$WINDOWS_DIR/mpv-dev/mpv.def"
    if command -v lib.exe &>/dev/null; then
        (cd "$WINDOWS_DIR/mpv-dev" && lib.exe /def:mpv.def /name:libmpv-2.dll /out:mpv.lib /MACHINE:X64)
        MPV_LIB="$WINDOWS_DIR/mpv-dev/mpv.lib"
    elif command -v llvm-lib &>/dev/null; then
        (cd "$WINDOWS_DIR/mpv-dev" && llvm-lib /def:mpv.def /name:libmpv-2.dll /out:mpv.lib /MACHINE:X64)
        MPV_LIB="$WINDOWS_DIR/mpv-dev/mpv.lib"
    fi
fi

if [[ -z "$MPV_LIB" || ! -f "$MPV_LIB" ]]; then
    echo "ERROR: Could not find or generate mpv.lib for MSVC linking" >&2
    exit 1
fi

cp "$TEMP_DIR/service/stremio-runtime.exe" "$WINDOWS_DIR/resources/stremio-runtime.exe"
cp "$TEMP_DIR/service/server.js" "$WINDOWS_DIR/resources/server.cjs"
cp "$FFMPEG_EXE" "$WINDOWS_DIR/resources/ffmpeg.exe"
cp "$FFPROBE_EXE" "$WINDOWS_DIR/resources/ffprobe.exe"
cp "$LIBMPV_DLL" "$WINDOWS_DIR/resources/libmpv-2.dll"
cp "$MPV_LIB" "$WINDOWS_DIR/mpv-dev/mpv.lib"
[[ -n "$MPV_DEF" ]] && cp "$MPV_DEF" "$WINDOWS_DIR/mpv-dev/mpv.def"

echo "==> Windows shell dependencies ready under $WINDOWS_DIR"
