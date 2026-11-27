#!/usr/bin/env bash
# Build DROP as a Flatpak and export a single-file bundle.
#
#   bash scripts/build-flatpak.sh          # artifacts/HL100Drop-<version>-x86_64.flatpak
#   bash scripts/build-flatpak.sh --install  # also install it for the current user
#
# Requirements: flatpak, flatpak-builder and the Flathub remote
# (flatpak remote-add --user --if-not-exists flathub https://dl.flathub.org/repo/flathub.flatpakrepo).
# The GNOME runtime and the Rust SDK extension are installed automatically.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"
version="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' apps/linux/Cargo.toml | head -1)}"
out="$root/artifacts"
manifest="apps/linux/flatpak/ru.hl100.drop.yml"
bundle="$out/HL100Drop-${version}-x86_64.flatpak"
mkdir -p "$out"

install_flag=()
if [ "${1:-}" = "--install" ]; then
    install_flag=(--install)
fi

flatpak-builder --user --force-clean --install-deps-from=flathub \
    --state-dir="$out/.flatpak-builder" \
    --repo="$out/flatpak-repo" \
    "${install_flag[@]}" \
    "$out/flatpak-build" "$manifest"

# The bundle pulls the GNOME runtime from Flathub on first install.
flatpak build-bundle \
    --runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo \
    "$out/flatpak-repo" "$bundle" ru.hl100.drop
echo "Готово: $bundle"
