#!/bin/bash
# Runs once as root on the test box. Progress goes to the instance console, so
# `aws ec2 get-console-output` shows it without a login.
# No `set -x`: it printed the password and the presigned package links to the console.
exec > >(tee -a /var/log/box-setup.log /dev/console) 2>&1
set -u
export DEBIAN_FRONTEND=noninteractive
step() { echo "[box-setup] $*"; }

DCV_URL=https://d1uj6qtbmh3dt5.cloudfront.net/2025.0/Servers/nice-dcv-2025.0-20103-ubuntu2404-x86_64.tgz
DCV_SHA256=a39374d39f2d849bd13ee101970bb9eea15a8c5ec743799b7cbb7f562ece9e17
HOME_T=/home/tester
DESK=$HOME_T/Desktop
BIN=$HOME_T/bin
PKG=$HOME_T/packages

step "installing desktop packages"
apt-get update -y
apt-get install -y --no-install-recommends \
  xfce4-session xfwm4 xfce4-panel xfdesktop4 thunar xfce4-terminal dbus-x11 \
  libfuse2t64 flatpak mpv curl ca-certificates ssl-cert xdg-utils libglib2.0-bin \
  fonts-noto-core fonts-noto-color-emoji x11-utils x11-xserver-utils xauth squashfs-tools

step "installing Amazon DCV"
# No license server is needed on EC2, but the instance role must be able to read the DCV license bucket.
curl -fsSL --retry 5 "$DCV_URL" -o /tmp/dcv.tgz
echo "$DCV_SHA256  /tmp/dcv.tgz" | sha256sum -c - || { echo "DCV CHECKSUM MISMATCH"; exit 1; }
mkdir -p /tmp/dcv && tar -xzf /tmp/dcv.tgz -C /tmp/dcv --strip-components=1
apt-get install -y /tmp/dcv/nice-dcv-server_*.deb /tmp/dcv/nice-dcv-web-viewer_*.deb /tmp/dcv/nice-xdcv_*.deb
usermod -aG video dcv

id tester >/dev/null 2>&1 || useradd -m -s /bin/bash -G sudo,ssl-cert tester
echo 'tester ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/tester
mkdir -p "$DESK" "$BIN" "$PKG" "$HOME_T/.config/autostart"

step "downloading the packages"
curl -fsSL --retry 5 "$URL_APPIMAGE" -o "$PKG/Stremio_Lightning.AppImage"
curl -fsSL --retry 5 "$URL_DEB"      -o "$PKG/stremio-lightning.deb"
curl -fsSL --retry 5 "$URL_FLATPAK"  -o "$PKG/stremio-lightning.flatpak"
chmod +x "$PKG/Stremio_Lightning.AppImage"
sha256sum "$PKG"/* > "$DESK/PACKAGE-HASHES.txt"

step "writing launchers"
# Software rendering: this box has no GPU.
cat > "$HOME_T/.dcv-init" <<'XEOF'
#!/bin/sh
export LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
# Portals started by systemd need DISPLAY, or the app waits ~25 s per portal.
systemctl --user import-environment DISPLAY XAUTHORITY || true
dbus-update-activation-environment --systemd DISPLAY XAUTHORITY || true
exec startxfce4
XEOF
chmod +x "$HOME_T/.dcv-init"

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

cat > "$BIN/show-logs.sh" <<'REOF'
#!/bin/bash
# Prints the end of the newest app log, so it can be copied from this window.
for d in "$HOME/.local/share/stremio-lightning/logs" "$HOME/.var/app/io.github.theguy000.stremio-lightning/data/stremio-lightning/logs"; do
  f=$(ls -t "$d"/* 2>/dev/null | head -1)
  echo "===== $d"
  if [ -n "$f" ]; then echo "file: $f"; tail -n 80 "$f"; else echo "(no log files)"; fi
done
read -r -p "Press Enter to close. "
REOF
cat > "$BIN/install-system-webkit.sh" <<'REOF'
#!/bin/bash
# Workaround only: installs the distro WebKitGTK so the AppImage and .deb can find their helper programs.
sudo apt-get install -y libwebkitgtk-6.0-4
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
mk_launcher show-logs "Show logs" show-logs.sh
mk_launcher install-system-webkit "Install system WebKit (workaround)" install-system-webkit.sh
chmod +x "$DESK"/*.desktop

cat > "$DESK/README.txt" <<'REOF'
Double-click a launcher: Run AppImage, Run .deb or Run Flatpak. It opens in a terminal window
and stays open after the app exits, so any error stays visible.
If a double-click does nothing, open the terminal from the panel and run one of these:
  ~/bin/run-appimage.sh
  ~/bin/run-deb.sh
  ~/bin/run-flatpak.sh
Close one app before starting the next. REPORT.txt is the automatic library check.
Show logs prints the newest app log. Copy it from that window if something fails.
Install system WebKit (workaround) is only a workaround: it installs the distro WebKitGTK. Use it
only to test past a crash on "WebKitNetworkProcess ... No such file or directory".
The box deletes itself on schedule.
REOF
chown -R tester:tester "$HOME_T"

step "starting the remote desktop"
echo "tester:$BOX_PASSWORD" | chpasswd
unset BOX_PASSWORD
systemctl enable --now dcvserver
sleep 5
dcv create-session --type=virtual --owner tester --user tester --name stremio --init "$HOME_T/.dcv-init" stremio || echo "DCV SESSION START FAILED"
dcv list-sessions
ss -ltnp | grep ':8443' || echo "NO LISTENER ON 8443"

step "running the library checks"
REPORT=$DESK/REPORT.txt
{
  echo "== kernel / sandbox"
  uname -r
  dcv list-sessions 2>&1
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
