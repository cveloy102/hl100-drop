# Validation — 0.1.0

Validated locally on 2026-10-13:

- Rust formatting, Clippy with warnings denied, and all five tests passed.
- Tests cover a real TCP tree transfer, binary/Unicode files, empty directories,
  name collisions, cancellation and interrupted-transfer cleanup.
- Windows self-contained x64 build and Inno Setup installer completed. Isolated
  silent installation and uninstallation both returned exit code 0.
- Android signed release builds for ARM64 and x86_64 completed; APK signature
  verification passed. Android debug lint passed during development.
- Signed Android app installed on a physical Poco F7 running Android 16.
- Windows discovered the phone on the same Wi-Fi LAN. Windows native C ABI sent
  a generated UTF-8 file to Android, with the receiver acknowledging storage
  finalization successfully.

Still pending before calling this a stable release:

- Android-to-Windows transfer and returned-file checksum on the physical phone.
- Large-file throughput, folders, cancellation and reconnect on physical devices.
- Windows drag-and-drop and file-picker interaction checks.
- Actual newer-release banner, download and OS installation on both platforms.
- Compatibility runs on Windows 10 and Android 12 (build minimums are configured).

The macOS application has not been implemented yet; see `apps/macos/README.md`.
