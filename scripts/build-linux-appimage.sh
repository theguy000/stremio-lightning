#!/bin/bash
# Builds the native Linux shell crate and packages it as an AppImage.

set -euo pipefail

APP_NAME="Stremio Lightning"
APP_ID="stremio-lightning"
BIN_NAME="stremio-lightning-linux"
LINUX_DIR="crates/stremio-lightning-linux"
ICON_PATH="assets/icons/128x128.png"
APPDIR="target/appimage/${APP_ID}.AppDir"
DIST_DIR="dist"
APPIMAGE_TOOL="${APPIMAGE_TOOL:-$HOME/.cache/appimage/appimagetool-x86_64.AppImage}"

required_file() {
    if [[ ! -s "$1" ]]; then
        echo "ERROR: Missing required file: $1" >&2
        echo "       Run: npm run setup:linux-shell" >&2
        exit 1
    fi
}

required_file "$LINUX_DIR/binaries/stremio-runtime-x86_64-unknown-linux-gnu"
required_file "$LINUX_DIR/resources/server.cjs"
required_file "$LINUX_DIR/resources/ffmpeg"
required_file "$LINUX_DIR/resources/ffprobe"

echo "==> Building native Linux shell crate..."
cargo build -p "$BIN_NAME" --release

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib/$APP_ID/binaries" "$APPDIR/usr/lib/$APP_ID/resources" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons/hicolor/128x128/apps" "$DIST_DIR"

cp "target/release/$BIN_NAME" "$APPDIR/usr/bin/$BIN_NAME"
cp "$LINUX_DIR/binaries/stremio-runtime-x86_64-unknown-linux-gnu" "$APPDIR/usr/lib/$APP_ID/binaries/"
cp "$LINUX_DIR/resources/server.cjs" "$APPDIR/usr/lib/$APP_ID/resources/"
cp "$LINUX_DIR/resources/ffmpeg" "$APPDIR/usr/lib/$APP_ID/resources/"
cp "$LINUX_DIR/resources/ffprobe" "$APPDIR/usr/lib/$APP_ID/resources/"
chmod +x "$APPDIR/usr/bin/$BIN_NAME" "$APPDIR/usr/lib/$APP_ID/binaries/stremio-runtime-x86_64-unknown-linux-gnu" "$APPDIR/usr/lib/$APP_ID/resources/ffmpeg" "$APPDIR/usr/lib/$APP_ID/resources/ffprobe"

if [[ -f "$ICON_PATH" ]]; then
    cp "$ICON_PATH" "$APPDIR/$APP_ID.png"
else
    echo "ERROR: Missing icon: $ICON_PATH" >&2
    exit 1
fi
cp "$APPDIR/$APP_ID.png" "$APPDIR/usr/share/icons/hicolor/128x128/apps/$APP_ID.png"

cat > "$APPDIR/$APP_ID.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$APP_NAME
Exec=$BIN_NAME
Icon=$APP_ID
Categories=AudioVideo;Video;Player;
Terminal=false
EOF
cp "$APPDIR/$APP_ID.desktop" "$APPDIR/usr/share/applications/$APP_ID.desktop"

cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/bash
set -euo pipefail
HERE=$(dirname "$(readlink -f "$0")")
export STREMIO_LIGHTNING_BUNDLE_DIR="$HERE/usr/lib/stremio-lightning"
exec "$HERE/usr/bin/stremio-lightning-linux" "$@"
EOF
chmod +x "$APPDIR/AppRun"

if [[ ! -x "$APPIMAGE_TOOL" ]]; then
    echo "ERROR: AppImage tool not found or not executable: $APPIMAGE_TOOL" >&2
    echo "       Set APPIMAGE_TOOL=/path/to/appimagetool or download appimagetool to the default cache path." >&2
    exit 1
fi

echo "==> Packaging AppImage..."
rm -f "$DIST_DIR/Stremio_Lightning_Linux-x86_64.AppImage"
(cd "$DIST_DIR" && ARCH=x86_64 "$APPIMAGE_TOOL" --appimage-extract-and-run --appdir="../$APPDIR")
mv "$DIST_DIR"/*.AppImage "$DIST_DIR/Stremio_Lightning_Linux-x86_64.AppImage"
chmod +x "$DIST_DIR/Stremio_Lightning_Linux-x86_64.AppImage"

echo "==> AppImage ready: $DIST_DIR/Stremio_Lightning_Linux-x86_64.AppImage"
