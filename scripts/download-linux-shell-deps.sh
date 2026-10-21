#!/bin/bash
# Downloads Linux runtime files for the native Linux shell crate.

set -euo pipefail

# The headless build of stream-server (no tray, TUI or settings window), mirrored
# at theguy000/stream-server. It is pinned by SHA-256; there is no fallback.
STREAM_SERVER_VERSION="v0.1.8-lightning.rc2"
STREAM_SERVER_SHA256="34de1e4cef846938d6d652597ddd64c285ac24a1db52ced4332ec6b5bf32a0d5"
STREAM_SERVER_URL="https://github.com/theguy000/stream-server/releases/download/$STREAM_SERVER_VERSION/stream-server-linux-amd64-headless"

# Static FFmpeg from BtbN/FFmpeg-Builds, which publishes to GitHub releases.
# The previous source, johnvansickle.com, is not reachable from CI runners.
# BtbN ships unstripped binaries, so they are stripped below before packaging.
FFMPEG_RELEASE="autobuild-2026-10-20-13-08"
FFMPEG_ASSET="ffmpeg-n9.0.2-29-g6dfaba896f-linux64-gpl-9.0.tar.xz"
FFMPEG_SHA256="6a40dd0b5a020854573aa8bf11449694fabd4fdff1be09b43f6713ca8a2abab1"
FFMPEG_LINUX_URL="https://github.com/BtbN/FFmpeg-Builds/releases/download/$FFMPEG_RELEASE/$FFMPEG_ASSET"

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

echo "==> Downloading static FFmpeg $FFMPEG_RELEASE (Linux)..."
if ! curl -fL "$FFMPEG_LINUX_URL" -o "$TEMP_DIR/ffmpeg-linux.tar.xz"; then
    echo "ERROR: Failed to download FFmpeg from $FFMPEG_LINUX_URL" >&2
    exit 1
fi
actual_ffmpeg_sha=$(sha256sum "$TEMP_DIR/ffmpeg-linux.tar.xz" | awk '{print $1}')
if [[ "$actual_ffmpeg_sha" != "$FFMPEG_SHA256" ]]; then
    echo "ERROR: FFmpeg checksum mismatch (expected $FFMPEG_SHA256, got $actual_ffmpeg_sha)" >&2
    exit 1
fi

mkdir -p "$TEMP_DIR/ffmpeg-linux"
tar -xf "$TEMP_DIR/ffmpeg-linux.tar.xz" -C "$TEMP_DIR/ffmpeg-linux"

FFMPEG_BIN=$(find "$TEMP_DIR/ffmpeg-linux" -type f -name ffmpeg -print -quit)
FFPROBE_BIN=$(dirname "$FFMPEG_BIN")/ffprobe

if [[ ! -f "$FFMPEG_BIN" || ! -f "$FFPROBE_BIN" ]]; then
    echo "ERROR: Could not find ffmpeg or ffprobe in downloaded archive" >&2
    exit 1
fi

# Upstream binaries are unstripped, at roughly 173 MB each.
if command -v strip >/dev/null 2>&1; then
    strip --strip-all "$FFMPEG_BIN" "$FFPROBE_BIN"
else
    echo "WARNING: strip not found, packaging unstripped FFmpeg" >&2
fi

cp "$TEMP_DIR/stream-server" "$LINUX_DIR/resources/stream-server"
cp "$FFMPEG_BIN" "$LINUX_DIR/resources/ffmpeg"
cp "$FFPROBE_BIN" "$LINUX_DIR/resources/ffprobe"
chmod +x "$LINUX_DIR/resources/stream-server" "$LINUX_DIR/resources/ffmpeg" "$LINUX_DIR/resources/ffprobe"

echo "==> Linux shell dependencies ready:"
echo "    $LINUX_DIR/resources/stream-server"
echo "    $LINUX_DIR/resources/ffmpeg"
echo "    $LINUX_DIR/resources/ffprobe"
