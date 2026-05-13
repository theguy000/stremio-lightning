#!/bin/bash
set -euo pipefail

readonly BUNDLE_URL="https://github.com/theguy000/stremio-lightning/releases/latest/download/Stremio_Lightning_Linux-x86_64.flatpak"

tmp="$(mktemp --suffix=.flatpak)"
trap 'rm -f "$tmp"' EXIT

curl -L -o "$tmp" "$BUNDLE_URL"
flatpak remote-add --user --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
flatpak install --user --bundle "$tmp" -y
