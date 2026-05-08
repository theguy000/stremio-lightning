#!/bin/bash
# Downloads Linux runtime files for the native Linux shell crate.

set -euo pipefail

SERVICE_VERSION="v0.1.21"
SERVICE_REPO="Stremio/stremio-service"
FFMPEG_LINUX_URL="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"
LINUX_DIR="crates/stremio-lightning-linux"
TEMP_DIR=$(mktemp -d)

cleanup() {
    rm -rf "$TEMP_DIR"
}
trap cleanup EXIT

mkdir -p "$LINUX_DIR/binaries" "$LINUX_DIR/resources"

echo "==> Downloading stremio-service $SERVICE_VERSION (Linux)..."
gh release download "$SERVICE_VERSION" --repo "$SERVICE_REPO" --pattern "stremio-service_amd64.deb" --dir "$TEMP_DIR"

mkdir -p "$TEMP_DIR/deb-extracted"
dpkg-deb -x "$TEMP_DIR/stremio-service_amd64.deb" "$TEMP_DIR/deb-extracted"

RUNTIME_BIN=$(find "$TEMP_DIR/deb-extracted" -type f -name "stremio-runtime" | head -n 1)
SERVER_JS=$(find "$TEMP_DIR/deb-extracted" -type f -name "server.js" | head -n 1)

if [[ -z "$RUNTIME_BIN" || -z "$SERVER_JS" ]]; then
    echo "ERROR: Could not find stremio-runtime or server.js in stremio-service deb" >&2
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

cp "$RUNTIME_BIN" "$LINUX_DIR/binaries/stremio-runtime-x86_64-unknown-linux-gnu"
cp "$SERVER_JS" "$LINUX_DIR/resources/server.cjs"
cp "$FFMPEG_BIN" "$LINUX_DIR/resources/ffmpeg"
cp "$FFPROBE_BIN" "$LINUX_DIR/resources/ffprobe"
chmod +x "$LINUX_DIR/binaries/stremio-runtime-x86_64-unknown-linux-gnu" "$LINUX_DIR/resources/ffmpeg" "$LINUX_DIR/resources/ffprobe"

echo "==> Linux shell dependencies ready:"
echo "    $LINUX_DIR/binaries/stremio-runtime-x86_64-unknown-linux-gnu"
echo "    $LINUX_DIR/resources/server.cjs"
echo "    $LINUX_DIR/resources/ffmpeg"
echo "    $LINUX_DIR/resources/ffprobe"
