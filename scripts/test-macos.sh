#!/bin/bash
set -euo pipefail
task_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$task_root"
if ! command -v cargo >/dev/null && [[ -x .tools/cargo/bin/cargo ]]; then
    export CARGO_HOME="$task_root/.tools/cargo" RUSTUP_HOME="$task_root/.tools/rustup"
    export PATH="$CARGO_HOME/bin:$PATH"
fi
cargo test --workspace --locked
task_arch="$(uname -m)"
task_build="$task_root/apps/macos/build/tests"
mkdir -p "$task_build"
xcrun swiftc -swift-version 5 -target "$task_arch-apple-macosx12.0" \
    -import-objc-header core/include/hl100.h -L artifacts/macos/'HL100 Drop.app'/Contents/Frameworks -lhl100_core \
    -Xlinker -rpath -Xlinker "$task_root/artifacts/macos/HL100 Drop.app/Contents/Frameworks" \
    apps/macos/Sources/Engine.swift apps/macos/Sources/Updates.swift apps/macos/Sources/LoginLaunch.swift apps/macos/Tests/main.swift \
    -o "$task_build/smoke"
HL100_SETTINGS_PATH="$task_build/settings.json" "$task_build/smoke"
if [[ "${1:-}" == --ui ]]; then
    HL100_UI_SMOKE_PATH="$task_build/compact.png" \
        "$task_root/artifacts/macos/HL100 Drop.app/Contents/MacOS/HL100Drop" --ui-smoke --interaction-smoke
fi
