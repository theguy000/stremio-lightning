#!/bin/bash
# Runs once as root on the test box. Progress goes to the instance console, so
# `aws ec2 get-console-output` shows it without a login.
# No `set -x`: it printed the password and the presigned package links to the console.
exec > >(tee -a /var/log/box-setup.log /dev/console) 2>&1
set -u
export DEBIAN_FRONTEND=noninteractive
step() { echo "[box-setup] $*"; }

KASM_URL=https://github.com/kasmtech/KasmVNC/releases/download/v1.5.0/kasmvncserver_noble_1.5.0_amd64.deb
KASM_SHA256=f599fe02e2175b9817b6165f74a5d2bebdc73118dde9181ba3410963bed7ae1e
HOME_T=/home/tester
DESK=$HOME_T/Desktop
BIN=$HOME_T/bin
PKG=$HOME_T/packages

step "installing desktop packages"
apt-get update -y
apt-get install -y --no-install-recommends \
  xfce4-session xfwm4 xfce4-panel xfdesktop4 thunar xfce4-terminal dbus-x11 \
  libfuse2t64 flatpak mpv curl ca-certificates ssl-cert xdg-utils libglib2.0-bin \
  fonts-noto-core fonts-noto-color-emoji x11-utils squashfs-tools

step "installing KasmVNC"
curl -fsSL --retry 5 "$KASM_URL" -o /tmp/kasmvnc.deb
echo "$KASM_SHA256  /tmp/kasmvnc.deb" | sha256sum -c - || { echo "KASMVNC CHECKSUM MISMATCH"; exit 1; }
apt-get install -y /tmp/kasmvnc.deb

id tester >/dev/null 2>&1 || useradd -m -s /bin/bash -G sudo,ssl-cert tester
echo 'tester ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/tester
mkdir -p "$DESK" "$BIN" "$PKG" "$HOME_T/.config/autostart" "$HOME_T/.vnc"

step "downloading the packages"
curl -fsSL --retry 5 "$URL_APPIMAGE" -o "$PKG/Stremio_Lightning.AppImage"
curl -fsSL --retry 5 "$URL_DEB"      -o "$PKG/stremio-lightning.deb"
curl -fsSL --retry 5 "$URL_FLATPAK"  -o "$PKG/stremio-lightning.flatpak"
chmod +x "$PKG/Stremio_Lightning.AppImage"
sha256sum "$PKG"/* > "$DESK/PACKAGE-HASHES.txt"

step "writing launchers"
# Software rendering: this box has no GPU.
cat > "$HOME_T/.vnc/xstartup" <<'XEOF'
#!/bin/sh
export LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
exec startxfce4
XEOF
chmod +x "$HOME_T/.vnc/xstartup"

cat > "$BIN/run-appimage.sh" <<'REOF'
#!/bin/bash
export LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
"$HOME/packages/Stremio_Lightning.AppImage"
rc=$?
echo; echo "Exited with code $rc. Logs: ~/.local/share/stremio-lightning/logs/"
read -r -p "Press Enter to close. "
REOF
cat > "$BIN/run-deb.sh" <<'REOF'
#!/bin/bash
export LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
command -v stremio-lightning >/dev/null || sudo apt-get install -y "$HOME/packages/stremio-lightning.deb"
stremio-lightning
rc=$?
echo; echo "Exited with code $rc. Logs: ~/.local/share/stremio-lightning/logs/"
read -r -p "Press Enter to close. "
REOF
cat > "$BIN/run-flatpak.sh" <<'REOF'
#!/bin/bash
if ! flatpak info io.github.theguy000.stremio-lightning >/dev/null 2>&1; then
  sudo flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
  sudo flatpak install -y --noninteractive --system --bundle "$HOME/packages/stremio-lightning.flatpak"
fi
flatpak run io.github.theguy000.stremio-lightning
rc=$?
echo; echo "Exited with code $rc. Logs: ~/.var/app/io.github.theguy000.stremio-lightning/"
read -r -p "Press Enter to close. "
REOF

# Marks the Desktop launchers as trusted once the desktop session is up, so a double-click runs them.
cat > "$BIN/trust-launchers.sh" <<'REOF'
#!/bin/sh
sleep 6
for f in "$HOME"/Desktop/*.desktop; do
  chmod +x "$f"
  gio set "$f" metadata::xfce-exe-checksum "$(sha256sum "$f" | cut -d' ' -f1)" 2>/dev/null
  gio set "$f" metadata::trusted true 2>/dev/null
done
xfdesktop --reload 2>/dev/null
REOF
chmod +x "$BIN"/*.sh

cat > "$HOME_T/.config/autostart/trust-launchers.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Trust launchers
Exec=$BIN/trust-launchers.sh
Terminal=false
EOF

mk_launcher() {
  cat > "$DESK/$1.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=$2
Exec=$BIN/$3
Terminal=true
Icon=utilities-terminal
EOF
}
mk_launcher run-appimage "Run AppImage" run-appimage.sh
mk_launcher run-deb "Run .deb" run-deb.sh
mk_launcher run-flatpak "Run Flatpak" run-flatpak.sh
chmod +x "$DESK"/*.desktop

cat > "$DESK/README.txt" <<'REOF'
Double-click a launcher: Run AppImage, Run .deb or Run Flatpak. It opens in a terminal window
and stays open after the app exits, so any error stays visible.
If a double-click does nothing, open the terminal from the panel and run one of these:
  ~/bin/run-appimage.sh
  ~/bin/run-deb.sh
  ~/bin/run-flatpak.sh
Close one app before starting the next. REPORT.txt is the automatic library check.
The box deletes itself on schedule.
REOF
chown -R tester:tester "$HOME_T"

step "starting the remote desktop"
VNCPW=$(command -v kasmvncpasswd || command -v vncpasswd)
printf '%s\n%s\n' "$BOX_PASSWORD" "$BOX_PASSWORD" | su - tester -c "$VNCPW -u tester -w -r"
unset BOX_PASSWORD
su - tester -c "vncserver :1 -select-de xfce -geometry 1280x720 -depth 24" || echo "VNCSERVER START FAILED"
sleep 3
ss -ltnp | grep -E ':8444|:5901' || echo "NO LISTENER ON 8444"

step "running the library checks"
REPORT=$DESK/REPORT.txt
{
  echo "== kernel / sandbox"
  uname -r
  sysctl kernel.apparmor_restrict_unprivileged_userns kernel.unprivileged_userns_clone 2>&1
  echo
  echo "== AppImage: libraries of the bundled stream-server"
  mkdir -p /tmp/ai && cd /tmp/ai && "$PKG/Stremio_Lightning.AppImage" --appimage-extract >/dev/null 2>&1
  S=$(find /tmp/ai/squashfs-root -name stream-server -type f | head -1)
  echo "binary: $S"
  if [ -n "$S" ]; then ldd "$S" | grep 'not found' || echo "all libraries found"; else echo "stream-server NOT FOUND in AppImage"; fi
  echo
  echo "== deb: install and check libraries"
  apt-get install -y "$PKG/stremio-lightning.deb" 2>&1 | tail -3
  D=$(dpkg -L "$(dpkg-deb -f "$PKG/stremio-lightning.deb" Package)" | grep -E '/stream-server$' | head -1)
  echo "binary: $D"
  if [ -n "$D" ]; then ldd "$D" | grep 'not found' || echo "all libraries found"; else echo "stream-server NOT FOUND in deb"; fi
  echo
  echo "== flatpak: install and check libraries inside the sandbox"
  flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo 2>&1 | tail -2
  flatpak install -y --noninteractive --system --bundle "$PKG/stremio-lightning.flatpak" 2>&1 | tail -5
  flatpak run --command=sh io.github.theguy000.stremio-lightning -c 'S=$(find /app -name stream-server -type f | head -1); echo "binary: $S"; ldd "$S" | grep "not found" || echo "all libraries found"' 2>&1
  echo
  echo "== done $(date -u +%FT%TZ)"
} > "$REPORT" 2>&1
chown tester:tester "$REPORT"
cat "$REPORT"
echo "BOX-READY"
