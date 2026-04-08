#!/bin/bash
# Downloads stremio-service release files for the Tauri sidecar.
# Run from the project root: bash src-tauri/scripts/download-server.sh

set -euo pipefail

VERSION="v0.1.21"
REPO="Stremio/stremio-service"
ASSET="stremio-service-windows.zip"
TAURI_DIR="src-tauri"
TEMP_DIR=$(mktemp -d)

echo "Downloading stremio-service $VERSION..."
gh release download "$VERSION" --repo "$REPO" --pattern "$ASSET" --dir "$TEMP_DIR"

echo "Extracting..."
mkdir -p "$TAURI_DIR/binaries" "$TAURI_DIR/resources"
unzip -o "$TEMP_DIR/$ASSET" -d "$TEMP_DIR/extracted"

# Move files to Tauri locations
cp "$TEMP_DIR/extracted/stremio-runtime.exe" "$TAURI_DIR/binaries/stremio-runtime-x86_64-pc-windows-msvc.exe"
cp "$TEMP_DIR/extracted/server.js" "$TAURI_DIR/resources/server.js"
cp "$TEMP_DIR/extracted/ffmpeg.exe" "$TAURI_DIR/resources/ffmpeg.exe"
cp "$TEMP_DIR/extracted/ffprobe.exe" "$TAURI_DIR/resources/ffprobe.exe"

rm -rf "$TEMP_DIR"

echo "Done! Files placed in:"
echo "  $TAURI_DIR/binaries/stremio-runtime-x86_64-pc-windows-msvc.exe"
echo "  $TAURI_DIR/resources/server.js"
echo "  $TAURI_DIR/resources/ffmpeg.exe"
echo "  $TAURI_DIR/resources/ffprobe.exe"
