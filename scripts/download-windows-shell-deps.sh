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

require_command() {
    local command_name="$1"

    if ! command -v "$command_name" &>/dev/null; then
        echo "ERROR: $command_name not found" >&2
        exit 1
    fi
}

first_match() {
    local root="$1"
    local name="$2"

    find "$root" -type f -name "$name" | sort | head -n 1
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

require_command gh
require_command curl
require_command unzip

rm -rf "$WINDOWS_DIR/resources" "$WINDOWS_DIR/mpv-dev"
mkdir -p "$WINDOWS_DIR/resources" "$WINDOWS_DIR/mpv-dev"

echo "==> Downloading stremio-service $SERVICE_VERSION (Windows)..."
gh release download "$SERVICE_VERSION" --repo "$SERVICE_REPO" --pattern "stremio-service-windows.zip" --dir "$TEMP_DIR"
unzip -o "$TEMP_DIR/stremio-service-windows.zip" -d "$TEMP_DIR/service"

STREMIO_RUNTIME_EXE=$(first_match "$TEMP_DIR/service" "stremio-runtime.exe")
SERVER_JS=$(first_match "$TEMP_DIR/service" "server.js")

if [[ -z "$STREMIO_RUNTIME_EXE" || -z "$SERVER_JS" ]]; then
    echo "ERROR: Could not find stremio-runtime.exe or server.js in downloaded service archive" >&2
    exit 1
fi

echo "==> Downloading static FFmpeg (Windows)..."
curl -L "$FFMPEG_WIN_URL" -o "$TEMP_DIR/ffmpeg.zip"
unzip -o "$TEMP_DIR/ffmpeg.zip" -d "$TEMP_DIR/ffmpeg"

FFMPEG_EXE=$(first_match "$TEMP_DIR/ffmpeg" "ffmpeg.exe")
FFPROBE_EXE=$(first_match "$TEMP_DIR/ffmpeg" "ffprobe.exe")

if [[ -z "$FFMPEG_EXE" || -z "$FFPROBE_EXE" ]]; then
    echo "ERROR: Could not find ffmpeg.exe or ffprobe.exe in downloaded archive" >&2
    exit 1
fi

echo "==> Downloading libmpv development files (Windows)..."
curl -L "$MPV_DEV_WIN_URL" -o "$TEMP_DIR/mpv-dev.7z"
extract_7z "$TEMP_DIR/mpv-dev.7z" "$TEMP_DIR/mpv-dev"

LIBMPV_DLL=$(first_match "$TEMP_DIR/mpv-dev" "libmpv-2.dll")
MPV_LIB=$(first_match "$TEMP_DIR/mpv-dev" "mpv.lib")
MPV_DEF=$(first_match "$TEMP_DIR/mpv-dev" "mpv.def")

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

cp "$STREMIO_RUNTIME_EXE" "$WINDOWS_DIR/resources/stremio-runtime.exe"
cp "$SERVER_JS" "$WINDOWS_DIR/resources/server.cjs"
cp "$FFMPEG_EXE" "$WINDOWS_DIR/resources/ffmpeg.exe"
cp "$FFPROBE_EXE" "$WINDOWS_DIR/resources/ffprobe.exe"
cp "$LIBMPV_DLL" "$WINDOWS_DIR/resources/libmpv-2.dll"
cp "$MPV_LIB" "$WINDOWS_DIR/mpv-dev/mpv.lib"
[[ -n "$MPV_DEF" ]] && cp "$MPV_DEF" "$WINDOWS_DIR/mpv-dev/mpv.def"

for required_file in \
    "$WINDOWS_DIR/resources/stremio-runtime.exe" \
    "$WINDOWS_DIR/resources/server.cjs" \
    "$WINDOWS_DIR/resources/ffmpeg.exe" \
    "$WINDOWS_DIR/resources/ffprobe.exe" \
    "$WINDOWS_DIR/resources/libmpv-2.dll" \
    "$WINDOWS_DIR/mpv-dev/mpv.lib"; do
    if [[ ! -f "$required_file" ]]; then
        echo "ERROR: Missing expected Windows shell dependency: $required_file" >&2
        exit 1
    fi
done

echo "==> Windows shell dependencies ready under $WINDOWS_DIR"
echo "==> Build.rs copies resources/libmpv-2.dll beside the Cargo-built executable on Windows"
