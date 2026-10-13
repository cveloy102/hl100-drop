# HL100 Drop — agreed requirements

- Native Windows 10+ (C#/WPF), Android 12+ (Kotlin/Compose); macOS Intel + Apple Silicon planned as Swift/AppKit.
- Shared Rust transfer/discovery engine, OS-specific storage and UI.
- Files and folders; recipient selected explicitly, automatic receive with no confirmation.
- First-run setup: device name and receive folder; both editable in settings.
- Desktop drag-and-drop to a recipient; file and folder picker also available.
- Progress, speed, cancel, batch queue through multiple transfer connections.
- No app-level accounts, passwords, pairing or transfer encryption.
- Style inspired by tg.hl100.ru: near-black background, warm white monospaced text, red accents, fine borders, rectangular controls.
- Windows per-user installer, Start menu shortcut, optional desktop shortcut, system uninstaller. Settings and received files survive updates/uninstall.
- GitHub Releases: startup check and settings button, persistent main-window banner, user-triggered verified download and OS installation. LAN exchange works offline. Only the updater uses internet.
- Android reception while the foreground service is running, with persistent notification; service starts from the visible app. No promise of unattended startup after boot or process termination.

Defaults selected for v1: one recipient per action, concurrent independent transfers supported, no resume, no multicast file distribution. These can be extended without changing the initial setup.

## Release naming

Tag `v0.1.0`, assets `HL100DropSetup-0.1.0.exe`, `HL100Drop-0.1.0.apk`, `checksums.txt` (SHA-256). APK distribution key must remain the same across all published updates. Do not publish debug APKs as production updates.

## Acceptance checks

Windows ↔ Android discovery and bidirectional transfer on a real LAN; nested/empty folders; Cyrillic filenames; zero-byte and large files; collision naming; cancellation/network loss; receive-directory access revoked/full storage; offline startup; updates with valid/invalid digest; upgrade keeping preferences; installer/uninstaller. macOS compatibility requires an actual Mac later.
