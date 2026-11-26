#!/usr/bin/env bash
# Build DROP for Linux: a .deb and, when linuxdeploy is available, an AppImage.
#
#   bash scripts/build-linux.sh            # version from apps/linux/Cargo.toml
#   LINUXDEPLOY=/path/linuxdeploy-x86_64.AppImage bash scripts/build-linux.sh
#
# Requirements: Rust, pkg-config, GTK 4.12+ development files, dpkg-deb.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
version="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' apps/linux/Cargo.toml | head -1)}"
arch="$(dpkg --print-architecture 2>/dev/null || echo amd64)"
out="$root/artifacts"
stage="$out/linux"
app_id="ru.hl100.drop"
data="apps/linux/data"

cargo build --release --locked -p hl100-drop
rm -rf "$stage"
mkdir -p "$stage"

# Files shared by the .deb and the AppImage, laid out under /usr.
install_tree() {
    local usr="$1"
    install -Dm755 target/release/hl100-drop "$usr/bin/hl100-drop"
    install -Dm644 "$data/$app_id.desktop" "$usr/share/applications/$app_id.desktop"
    install -Dm644 "$data/$app_id.svg" "$usr/share/icons/hicolor/scalable/apps/$app_id.svg"
    install -Dm644 "$data/$app_id.metainfo.xml" "$usr/share/metainfo/$app_id.metainfo.xml"
}

# ---- .deb (Debian, Ubuntu, Kali, Mint, Pop!_OS, elementary…) ----
deb="$stage/deb"
install_tree "$deb/usr"
# File manager "Send to" entries: Thunar (Xfce), Dolphin (KDE 5 and 6).
install -Dm644 "$data/thunar-sendto.desktop" "$deb/usr/share/Thunar/sendto/$app_id.desktop"
install -Dm644 "$data/kde-servicemenu.desktop" "$deb/usr/share/kio/servicemenus/$app_id.desktop"
install -Dm644 "$data/kde-servicemenu.desktop" "$deb/usr/share/kservices5/ServiceMenus/$app_id.desktop"
# Firewall profiles: `sudo ufw allow DROP`, `firewall-cmd --add-service=hl100-drop`.
install -Dm644 "$data/ufw-drop" "$deb/etc/ufw/applications.d/hl100-drop"
install -Dm644 "$data/firewalld-hl100-drop.xml" "$deb/usr/lib/firewalld/services/hl100-drop.xml"

glibc="$(objdump -T target/release/hl100-drop | grep -o 'GLIBC_[0-9.]*' | sed 's/GLIBC_//' | sort -Vu | tail -1)"
gtk="$(pkg-config --atleast-version=4.12 gtk4 && echo 4.12)"
size="$(du -sk "$deb" | cut -f1)"
mkdir -p "$deb/DEBIAN"
cat > "$deb/DEBIAN/control" <<EOF
Package: hl100-drop
Version: $version
Architecture: $arch
Maintainer: HL100 <noreply@github.com>
Installed-Size: $size
Depends: libc6 (>= $glibc), libgtk-4-1 (>= $gtk)
Recommends: xdg-desktop-portal-gtk | xdg-desktop-portal
Section: net
Priority: optional
Homepage: https://cveloy102.github.io/hl100-drop/
Description: DROP — передача файлов и папок по локальной сети
 Отправка файлов и папок между Linux, Windows, macOS и Android
 в одной доверенной локальной сети. Приём автоматически, докачка
 после обрыва связи, значок в трее. Передача без шифрования.
EOF
echo "/etc/ufw/applications.d/hl100-drop" > "$deb/DEBIAN/conffiles"
find "$deb" -type d -exec chmod 755 {} +
deb_file="$out/hl100-drop_${version}_${arch}.deb"
dpkg-deb --root-owner-group --build "$deb" "$deb_file" >/dev/null
echo "Готово: $deb_file"

# ---- AppImage (any distribution with glibc >= build host) ----
linuxdeploy="${LINUXDEPLOY:-$(command -v linuxdeploy-x86_64.AppImage || command -v linuxdeploy || true)}"
if [ -z "$linuxdeploy" ]; then
    echo "AppImage пропущен: укажите LINUXDEPLOY (и linuxdeploy-plugin-gtk в PATH)"
    exit 0
fi
appdir="$stage/AppDir"
install_tree "$appdir/usr"
image="$out/HL100Drop-${version}-x86_64.AppImage"
# The GTK plugin bundles GTK 4, its loaders and schemas, and sets GDK_BACKEND
# sensibly for both Wayland and X11 sessions.
DEPLOY_GTK_VERSION=4 OUTPUT="$image" NO_STRIP=1 "$linuxdeploy" \
    --appdir "$appdir" \
    --desktop-file "$appdir/usr/share/applications/$app_id.desktop" \
    --icon-file "$appdir/usr/share/icons/hicolor/scalable/apps/$app_id.svg" \
    --plugin gtk \
    --output appimage
echo "Готово: $image"
