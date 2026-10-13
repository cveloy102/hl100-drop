# Validation — 0.1.1

Validated locally on 2026-10-13:

- Rust formatting, Clippy with warnings denied, and all six tests passed.
- Tests cover a real TCP tree transfer, binary/Unicode files, empty directories,
  name collisions, cancellation and interrupted-transfer cleanup.
- A delayed sender regression test covers accepted sockets from a nonblocking
  listener. Transfer sockets explicitly use blocking mode with timeouts.
- Windows self-contained x64 build and Inno Setup installer completed. Isolated
  silent installation and uninstallation both returned exit code 0.
- Android signed release builds for ARM64 and x86_64 completed; APK signature
  verification passed. Android debug lint passed during development.
- Signed Android app installed on a physical Poco F7 running Android 16.
- Windows discovered the phone on the same Wi-Fi LAN. Windows native C ABI sent
  a generated UTF-8 file to Android, with the receiver acknowledging storage
  finalization successfully.
- Android returned that file through its native UI to Windows 0.1.1. SHA-256
  matched the original (53 bytes); an existing Windows file was preserved and
  the received copy was named with a numbered suffix.

Still pending before calling this a stable release:

- Large-file throughput, folders, cancellation and reconnect on physical devices.
- Windows drag-and-drop and file-picker interaction checks.
- Actual newer-release banner, download and OS installation on both platforms.
- Compatibility runs on Windows 10 and Android 12 (build minimums are configured).

## macOS implementation — 2026-10-13

- Swift/AppKit client and embedded Rust engine built for x86_64 and arm64,
  targeting macOS 12. Both Universal binaries checked with `lipo`.
- Embedded library loads via `@rpath` inside the application bundle.
- Six Rust regression tests passed on an Intel Mac.
- Swift C ABI smoke test passed with real loopback TCP send/receive of a
  multi-megabyte binary, Unicode names, zero-byte files, empty directories and
  duplicate folder names. Settings reload and update metadata validation passed.
- Compact menu-bar panel with `Hello, nearby_` rendered using the actual AppKit
  view hierarchy; settings are embedded in the same panel.
- AppKit interaction smoke test passed: status-icon drag entry opens the panel,
  icon drop stages files without sending, non-file pasteboard content is rejected,
  Pin prevents outside dismissal, an unpinned outside click closes the panel,
  dragging does not dismiss it, and the status icon toggles a pinned panel.
- Received filenames, sizes and collision-resolved local paths verified through
  real TCP transfers. Click actions in the panel and separate transfer window
  verified against the exact path; unfinished files cannot be revealed.
- Four native template menu-bar icons rendered and checked at small size.
  AppKit smoke test verifies icon selection/checkmarks, persisted preference and
  fallback to Bolt for unrecognized saved values.
- Three selectable title styles rendered: Lightning + DROP (default), Large DROP,
  Terminal; rejected red-DROP variant excluded.
- Login tests passed: isolated Monterey agent enable/disable/reload, app paths with
  spaces/Unicode, background flag and native login Apple-event detection.
  AppKit checks the checkbox, registration-error rollback and OS approval state;
  test fixtures do not alter the user's real login registrations.
- First-run settings view rendered and visually inspected. Universal app
  launches on Intel; bundle signatures and the generated DMG integrity verified.
  Distribution signing remains ad-hoc, without Apple notarization.

Still pending: physical M1 Pro execution; macOS 12 runtime checks; Windows and
Android LAN transfers; actual Finder drag/drop and picker interaction;
local-network/TCC permission behavior; actual logout/login and system approval
on macOS 12 and 13+; background reception and sleep/wake;
Developer ID signing/notarization and a real newer-version update.

See `apps/macos/README.md` for build, installation and signing instructions.

## Windows / Android — 0.1.2

Validated locally on 2026-10-13:

- All eight Rust tests, formatting and Clippy with warnings denied passed.
  Added regressions for EINTR retry and exact collision-resolved saved paths.
- Windows self-contained build and Inno Setup installer completed. Android
  signed release build and APK signature verification passed.
- Physical Poco F7 received the updated app. User confirmed Android -> Windows
  transfer and that discovery remained available without toggling reception.
  A ~3.5 MB JPEG arrived successfully; a ~5.3 MB PNG was sent back by drag/drop.
- Windows received generated native C ABI/TCP transfers. Received-file buttons
  show filenames/sizes and reveal the exact numbered copy in Explorer; selection
  of the collision-resolved file was visually verified.
- Embedded settings and themed scrollbar were visually inspected. Wheel
  scrolling changed the history scrollbar offset from 0 to 48.
- Login checkbox enabled a quoted executable command with --login in HKCU Run;
  disabling it removed only the application's value. Original off preference
  was restored. Duplicate normal/login launches retained one app process.
- Notification code compiled and transfer completion exercised. The desktop
  inspection tool did not enumerate the taskbar-hidden notification window,
  so its appearance and timing still need a manual check.
- The final APK was rebuilt after the saved-path fix. The phone was disconnected
  from ADB during final reinstall; that final artifact is available in the release.

No macOS binary rebuilt on this Windows host. Existing macOS verification above
belongs to the imported macOS revision; the updated core needs a Mac rebuild.
Actual Windows logout/login and stable-release update remain untested.
