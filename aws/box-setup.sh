#!/bin/bash
# Runs once as root on the test box. Everything is also written to the instance console,
# so `aws ec2 get-console-output` shows progress without a login.
exec > >(tee -a /var/log/box-setup.log /dev/console) 2>&1
set -x
export DEBIAN_FRONTEND=noninteractive

KASM_URL=https://github.com/kasmtech/KasmVNC/releases/download/v1.5.0/kasmvncserver_noble_1.5.0_amd64.deb
KASM_SHA256=f599fe02e2175b9817b6165f74a5d2bebdc73118dde9181ba3410963bed7ae1e
HOME_T=/home/tester
DESK=$HOME_T/Desktop
PKG=$HOME_T/packages

apt-get update -y
apt-get install -y --no-install-recommends \
  xfce4-session xfwm4 xfce4-panel xfdesktop4 thunar xfce4-terminal dbus-x11 \
  libfuse2t64 flatpak mpv curl ca-certificates ssl-cert xdg-utils \
  fonts-noto-core fonts-noto-color-emoji x11-utils squashfs-tools

curl -fsSL --retry 5 "$KASM_URL" -o /tmp/kasmvnc.deb
echo "$KASM_SHA256  /tmp/kasmvnc.deb" | sha256sum -c - || { echo "KASMVNC CHECKSUM MISMATCH"; exit 1; }
apt-get install -y /tmp/kasmvnc.deb

id tester >/dev/null 2>&1 || useradd -m -s /bin/bash -G sudo,ssl-cert tester
echo 'tester ALL=(ALL) NOPASSWD:ALL' > /etc/sudoers.d/tester
mkdir -p "$DESK" "$PKG"

curl -fsSL --retry 5 "$URL_APPIMAGE" -o "$PKG/Stremio_Lightning.AppImage"
curl -fsSL --retry 5 "$URL_DEB"      -o "$PKG/stremio-lightning.deb"
curl -fsSL --retry 5 "$URL_FLATPAK"  -o "$PKG/stremio-lightning.flatpak"
chmod +x "$PKG/Stremio_Lightning.AppImage"
sha256sum "$PKG"/* > "$DESK/PACKAGE-HASHES.txt"

# Software rendering: this box has no GPU.
mkdir -p "$HOME_T/.vnc"
cat > "$HOME_T/.vnc/xstartup" <<'XEOF'
#!/bin/sh
export LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
exec startxfce4
XEOF
chmod +x "$HOME_T/.vnc/xstartup"

cat > "$DESK/run-appimage.sh" <<'REOF'
#!/bin/bash
export LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
exec ~/packages/Stremio_Lightning.AppImage
REOF
cat > "$DESK/run-deb.sh" <<'REOF'
#!/bin/bash
export LIBGL_ALWAYS_SOFTWARE=1 WEBKIT_DISABLE_COMPOSITING_MODE=1
sudo apt-get install -y ~/packages/stremio-lightning.deb && exec stremio-lightning
REOF
cat > "$DESK/run-flatpak.sh" <<'REOF'
#!/bin/bash
sudo flatpak remote-add --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo
sudo flatpak install -y --noninteractive --system --bundle ~/packages/stremio-lightning.flatpak
exec flatpak run io.github.theguy000.stremio-lightning
REOF
cat > "$DESK/README.txt" <<'REOF'
Open the xfce4-terminal from the panel, then run one of these on the Desktop:
  ./run-appimage.sh   AppImage
  ./run-deb.sh        .deb
  ./run-flatpak.sh    Flatpak
REPORT.txt is the automatic library check. The box deletes itself on schedule.
Logs after a run: ~/.local/share/stremio-lightning/logs/
REOF
chmod +x "$DESK"/*.sh
chown -R tester:tester "$HOME_T"

VNCPW=$(command -v kasmvncpasswd || command -v vncpasswd)
su - tester -c "printf '%s\n%s\n' '$BOX_PASSWORD' '$BOX_PASSWORD' | $VNCPW -u tester -w -r"
su - tester -c "vncserver :1 -select-de xfce -geometry 1600x900 -depth 24" || echo "VNCSERVER START FAILED"
sleep 3
ss -ltnp | grep -E ':8444|:5901' || echo "NO LISTENER ON 8444"

# Automatic checks. They can be slow, so they run after the desktop is up.
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
