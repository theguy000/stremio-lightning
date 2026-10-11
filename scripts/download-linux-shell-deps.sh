#!/bin/bash
# Downloads Linux runtime files for the native Linux shell crate.

set -euo pipefail

# The headless build of stream-server (no tray, TUI or settings window), mirrored
# at theguy000/stream-server. It is pinned by SHA-256; there is no fallback.
STREAM_SERVER_VERSION="v0.1.8-lightning.rc2"
STREAM_SERVER_SHA256="34de1e4cef846938d6d652597ddd64c285ac24a1db52ced4332ec6b5bf32a0d5"
STREAM_SERVER_URL="https://github.com/theguy000/stream-server/releases/download/$STREAM_SERVER_VERSION/stream-server-linux-amd64-headless"
FFMPEG_LINUX_URL="https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz"
LINUX_DIR="crates/stremio-lightning-linux"
TEMP_DIR=$(mktemp -d)

cleanup() {
    rm -rf "$TEMP_DIR"
}
trap cleanup EXIT

mkdir -p "$LINUX_DIR/resources"

echo "==> Downloading stream-server $STREAM_SERVER_VERSION (Linux)..."
if ! curl -fsSL "$STREAM_SERVER_URL" -o "$TEMP_DIR/stream-server"; then
    echo "ERROR: Failed to download stream-server from $STREAM_SERVER_URL" >&2
    exit 1
fi
actual_sha=$(sha256sum "$TEMP_DIR/stream-server" | awk '{print $1}')
if [[ "$actual_sha" != "$STREAM_SERVER_SHA256" ]]; then
    echo "ERROR: stream-server checksum mismatch (expected $STREAM_SERVER_SHA256, got $actual_sha)" >&2
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

cp "$TEMP_DIR/stream-server" "$LINUX_DIR/resources/stream-server"
cp "$FFMPEG_BIN" "$LINUX_DIR/resources/ffmpeg"
cp "$FFPROBE_BIN" "$LINUX_DIR/resources/ffprobe"
chmod +x "$LINUX_DIR/resources/stream-server" "$LINUX_DIR/resources/ffmpeg" "$LINUX_DIR/resources/ffprobe"

echo "==> Linux shell dependencies ready:"
echo "    $LINUX_DIR/resources/stream-server"
echo "    $LINUX_DIR/resources/ffmpeg"
echo "    $LINUX_DIR/resources/ffprobe"
