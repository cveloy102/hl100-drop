#!/bin/bash
set -euo pipefail
task_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$task_root"
export MACOSX_DEPLOYMENT_TARGET=12.0
# A repository-local Rust installation is optional; regular rustup works too.
if ! command -v cargo >/dev/null && [[ -x "$task_root/.tools/cargo/bin/cargo" ]]; then
    export CARGO_HOME="$task_root/.tools/cargo"
    export RUSTUP_HOME="$task_root/.tools/rustup"
    export PATH="$CARGO_HOME/bin:$PATH"
fi
command -v cargo >/dev/null || { echo "Install Rust with rustup first." >&2; exit 1; }
task_version="$(sed -n 's/^version = "\([^"]*\)"/\1/p' core/Cargo.toml | head -1)"
task_output="$task_root/artifacts/macos"
task_app="$task_output/HL100 Drop.app"
task_build="$task_root/apps/macos/build"
task_sdk="$(xcrun --sdk macosx --show-sdk-path)"
mkdir -p "$task_build" "$task_app/Contents/"{MacOS,Frameworks,Resources}
rustup target add x86_64-apple-darwin aarch64-apple-darwin
for task_target in x86_64-apple-darwin aarch64-apple-darwin; do
    cargo build --locked --release -p hl100_core --target "$task_target"
done
xcrun lipo -create target/{x86_64-apple-darwin,aarch64-apple-darwin}/release/libhl100_core.dylib \
    -output "$task_app/Contents/Frameworks/libhl100_core.dylib"
xcrun install_name_tool -id '@rpath/libhl100_core.dylib' "$task_app/Contents/Frameworks/libhl100_core.dylib"
for task_arch in x86_64 arm64; do
    xcrun swiftc -swift-version 5 -O -whole-module-optimization -sdk "$task_sdk" -target "$task_arch-apple-macosx12.0" \
        -module-cache-path "$task_build/module-cache" \
        -import-objc-header "$task_root/core/include/hl100.h" \
        -L "$task_app/Contents/Frameworks" -lhl100_core \
        -Xlinker -rpath -Xlinker '@executable_path/../Frameworks' \
        apps/macos/Sources/*.swift -o "$task_build/HL100Drop-$task_arch"
done
xcrun lipo -create "$task_build/HL100Drop-"{x86_64,arm64} -output "$task_app/Contents/MacOS/HL100Drop"
cp apps/macos/Resources/Lucide-LICENSE "$task_app/Contents/Resources/Lucide-LICENSE"
cp apps/macos/Resources/Info.plist "$task_app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $task_version" "$task_app/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion ${BUILD_NUMBER:-1}" "$task_app/Contents/Info.plist"
xcrun swift -module-cache-path "$task_build/module-cache" scripts/macos-icon.swift "$task_build/Drop.iconset"
xcrun iconutil -c icns "$task_build/Drop.iconset" -o "$task_app/Contents/Resources/Drop.icns"
task_identity="${MACOS_SIGN_IDENTITY:--}"
for task_sign_path in "$task_app/Contents/Frameworks/libhl100_core.dylib" "$task_app"; do
    if [[ "$task_identity" == - ]]; then
        codesign --force --sign - "$task_sign_path"
    else
        codesign --force --sign "$task_identity" --options runtime --timestamp "$task_sign_path"
    fi
done
codesign --verify --deep --strict "$task_app"
xcrun lipo "$task_app/Contents/MacOS/HL100Drop" -verify_arch x86_64 arm64
xcrun lipo "$task_app/Contents/Frameworks/libhl100_core.dylib" -verify_arch x86_64 arm64
task_stage="$(mktemp -d "$task_output/dmg.XXXXXX")"
trap 'rm -rf "$task_stage"' EXIT
ditto "$task_app" "$task_stage/HL100 Drop.app"
ln -s /Applications "$task_stage/Applications"
task_dmg="$task_root/artifacts/HL100Drop-$task_version-universal.dmg"
hdiutil create -volname "HL100 Drop" -srcfolder "$task_stage" -format UDZO -ov "$task_dmg"
if [[ -n "${MACOS_NOTARY_PROFILE:-}" ]]; then
    [[ "$task_identity" != - ]] || { echo "Notarization requires MACOS_SIGN_IDENTITY." >&2; exit 1; }
    xcrun notarytool submit "$task_dmg" --keychain-profile "$MACOS_NOTARY_PROFILE" --wait
    xcrun stapler staple "$task_dmg"
fi
shasum -a 256 "$task_dmg" | sed "s|$task_root/artifacts/||" > "$task_output/checksums.txt"
echo "Built: $task_app"
echo "Installer: $task_dmg"
