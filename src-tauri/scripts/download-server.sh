#!/bin/bash
# Downloads stremio-service release files for the Tauri sidecar and replaces
# the broken ffmpeg/ffprobe executables from that archive with static builds.
# Run from the project root: bash src-tauri/scripts/download-server.sh

set -euo pipefail

VERSION="v0.1.21"
REPO="Stremio/stremio-service"
ASSET="stremio-service-windows.zip"
FFMPEG_URL="https://github.com/GyanD/codexffmpeg/releases/download/8.1/ffmpeg-8.1-essentials_build.7z"
TAURI_DIR="src-tauri"
TEMP_DIR=$(mktemp -d)

echo "Downloading stremio-service $VERSION..."
gh release download "$VERSION" --repo "$REPO" --pattern "$ASSET" --dir "$TEMP_DIR"

echo "Downloading static FFmpeg build..."
curl -L "$FFMPEG_URL" -o "$TEMP_DIR/ffmpeg-essentials.7z"

echo "Extracting..."
mkdir -p "$TAURI_DIR/binaries" "$TAURI_DIR/resources"
unzip -o "$TEMP_DIR/$ASSET" -d "$TEMP_DIR/extracted"
mkdir -p "$TEMP_DIR/ffmpeg"
7z x "$TEMP_DIR/ffmpeg-essentials.7z" -o"$TEMP_DIR/ffmpeg" -y

FFMPEG_BIN_DIR=$(find "$TEMP_DIR/ffmpeg" -type f -name ffmpeg.exe -exec dirname {} \; | head -n 1)

if [ -z "$FFMPEG_BIN_DIR" ]; then
	echo "Could not find ffmpeg.exe in downloaded FFmpeg archive" >&2
	exit 1
fi

# Move files to Tauri locations
cp "$TEMP_DIR/extracted/stremio-runtime.exe" "$TAURI_DIR/binaries/stremio-runtime-x86_64-pc-windows-msvc.exe"
cp "$TEMP_DIR/extracted/server.js" "$TAURI_DIR/resources/server.cjs"
cp "$FFMPEG_BIN_DIR/ffmpeg.exe" "$TAURI_DIR/resources/ffmpeg.exe"
cp "$FFMPEG_BIN_DIR/ffprobe.exe" "$TAURI_DIR/resources/ffprobe.exe"

rm -rf "$TEMP_DIR"

echo "Done! Files placed in:"
echo "  $TAURI_DIR/binaries/stremio-runtime-x86_64-pc-windows-msvc.exe"
echo "  $TAURI_DIR/resources/server.cjs"
echo "  $TAURI_DIR/resources/ffmpeg.exe"
echo "  $TAURI_DIR/resources/ffprobe.exe"
