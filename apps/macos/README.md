# macOS integration handoff

The native macOS UI is the next development stage. No macOS executable is included yet.

- Swift + AppKit, proposed deployment target macOS 12.
- Universal app: x86_64 Intel and arm64 Apple Silicon (including M1 Pro).
- Build the Rust core for `x86_64-apple-darwin` and `aarch64-apple-darwin`, combine dylibs using `lipo`, then embed/sign it with the app.
- Import `core/include/hl100.h` into Swift with a bridging header; decode JSON commands/events described in `docs/protocol.md`.
- Poll events on a worker queue, update AppKit views on the main thread.
- Use NSOpenPanel for the receive directory, NSDraggingDestination for dropped files, NSStatusItem for background reception.
- Check entitlements and local-network permission prompts on the macOS versions used for testing.
- Keep settings outside the application bundle. Preserve device ID and settings during update replacement.

The app and the core must both contain Intel and ARM slices. Test exchanges against Windows and Android on physical devices.
