# HL100 Drop

Native file sharing over a local network. Windows 10+, Android 12+, macOS 12+ Intel/Apple Silicon. Windows uses WPF, Android uses Jetpack Compose, macOS uses Swift/AppKit; all use the same Rust engine.

## Layout

- `core`: discovery, TCP streaming, file manifest, progress/cancellation, C ABI/JNI.
- `apps/windows`: native desktop UI, drag-and-drop, tray, settings, GitHub updater.
- `apps/android`: native phone UI, SAF storage, receive service, GitHub updater.
- `installer/windows`: Inno Setup per-user installer.
- `apps/macos`: native AppKit desktop UI, drag-and-drop, menu bar, settings and verified DMG updates.
- `docs`: agreed requirements and protocol.

## Windows build

Install Rust (MSVC + Visual Studio Build Tools, or GNU + GCC MinGW), .NET 8 SDK or newer and Inno Setup 6. Run:

```powershell
./scripts/build-windows.ps1
```

Output: `artifacts/HL100DropSetup-0.1.4.exe`. The published app includes its .NET runtime. Installation requires no administrator rights. The first connection may require the standard Windows Firewall prompt to allow local-network access.

## Android build

Install JDK, Android SDK 37, NDK r27d or newer, Rust targets `aarch64-linux-android` and `x86_64-linux-android`. Set `JAVA_HOME`, `ANDROID_HOME`, `ANDROID_NDK_HOME`, then:

```powershell
./scripts/build-android.ps1
```

This makes a debug APK for device testing. Production APKs require a permanent signing key; set `HL100_KEYSTORE`, `HL100_STORE_PASSWORD`, `HL100_KEY_ALIAS`, `HL100_KEY_PASSWORD`, then run with `-Release`. Keep the key out of Git and preserve it for future updates.

For a local signing key, run `scripts/initialize-signing.ps1` once. It saves the key and configuration in `private/` (excluded from Git). Back up both files privately. `build-android.ps1 -Release` reads this configuration automatically. Copy the resulting APK to `artifacts/HL100Drop-0.1.4.apk` before publishing.

## Core verification

```text
cargo test --workspace --locked
cargo fmt --all --check
```

Tests include a real loopback TCP transfer of a multi-megabyte binary with nested and empty folders, Unicode names and collision preservation. Real Windows/Android LAN validation and UI testing remain required before a stable release.

Tests also verify cancellation, removal of partial files after disconnect, and rejection of colliding manifest names. `scripts/smoke-ffi.ps1` sends a generated file through the DLL's C ABI to a running receiver. The `peer` Rust example is an optional local QA node; it exits after two minutes.

## Updates

Repository: https://github.com/cveloy102/hl100-drop. Checks use the latest stable GitHub Release, download only on user action and validate SHA-256 before launching an installer. File exchange requires no internet.

`scripts/publish-release.ps1` publishes built assets and SHA-256 checksums using the existing Git Credential Manager login. Releases are marked as prereleases by default; use `-Stable` only after device validation. Prereleases are available for manual download but do not trigger production update banners. Before a new version, increment the Rust package version, Windows project version and Android versionName/versionCode together.

## macOS build

Install Apple's Command Line Tools and Rust via rustup, then:

```sh
./scripts/build-macos.sh
./scripts/test-macos.sh
```

Output: `artifacts/HL100Drop-<core-version>-universal.dmg`. Drag the application to Applications.
Both the app and Rust dylib include Intel and ARM slices. Default builds use ad-hoc
signing for development; public distribution requires Developer ID signing and
notarization. See `apps/macos/README.md` for usage, release signing and validation.
