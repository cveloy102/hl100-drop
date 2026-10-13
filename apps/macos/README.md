# HL100 Drop for macOS

Native Swift/AppKit client for macOS 12 or newer. Both the application and the
shared Rust engine are Universal binaries containing `x86_64` and `arm64`.
Apple Silicon, including M1 Pro, uses the native ARM slice without Rosetta.

## Build and install

Install Apple's Command Line Tools (`xcode-select --install`) and Rust via
rustup. Full Xcode is optional for local builds. From the repository root:

```sh
./scripts/build-macos.sh
./scripts/test-macos.sh
```

Outputs:

- `artifacts/macos/HL100 Drop.app`
- `artifacts/HL100Drop-0.1.1-universal.dmg`
- `artifacts/macos/checksums.txt`

Open the DMG and drag the **HL100 Drop.app inside it** to **Applications**. Start it there. On the
first run, choose the device name and receive folder. Grant local-network and
folder access when macOS requests them. The firewall must allow incoming TCP
41000 and discovery UDP 41001. File exchange works without internet.

Click the menu-bar icon to open the compact panel. Select a recipient, then use
the send button to choose files/directories, or drag Finder items onto that
recipient's card. The shared drop zone sends to the selected device; without a
selection, it stages the files until you choose a recipient and click Send.
Dragging files over the menu-bar icon also opens the panel without activating
the application. Dropping on the icon stages files, never sends them immediately.
Independent transfers can run concurrently.
Incoming files are accepted automatically, as on Windows and Android. Use the
app on a trusted local network; protocol v1 has no authentication or encryption.

The panel closes on an outside click by default. **Pin** in the footer keeps
it open until the menu-bar icon is clicked again; the preference is remembered.
Finder drag sessions do not dismiss the panel. Escape closes an unpinned panel.
Closing the panel keeps reception running. The **•••** menu contains Settings,
updates, Quit and **Open transfer window**, a resizable view of the same history.
The **Icon** submenu offers Bolt (default), Radar, Nodes and Packet. Selection
applies immediately and is remembered across launches; all four marks are
monochrome template images that follow the macOS menu-bar appearance.
Each transfer shows offered filenames and sizes. Completed incoming files and
folders are clickable in both views: Finder opens with the actual saved item
selected, including collision-renamed items. Finished cards retain total size.
The interface lists the first 200 entries per transfer; larger batches show a
remaining count and the receive-folder shortcut. Already saved items remain
clickable if a later item fails. History is kept for the running session.
Use **Quit** in Settings or the application/status context menu to stop it;
active transfers require confirmation. Reception needs the app running and the
Mac awake. In Settings, **Launch DROP at login** enables background reception
without opening the panel. The checkbox changes the system registration immediately;
it is opt-in and off by default. macOS 13+ uses `SMAppService.mainApp` and displays
an approval button when the OS requires consent. macOS 12 uses a per-user
`~/Library/LaunchAgents/ru.hl100.drop.login.plist` (RunAtLoad, no KeepAlive),
which opens the installed app through LaunchServices with `--login`. Disabling
removes the registration for future logins and keeps the current app running.
Enable it after installing the app, not while running from the DMG.

Settings also offers three remembered title styles: **Lightning + DROP** (default),
**Large DROP**, and **Terminal**. The rejected red-DROP style is not included.
Appearance changes apply immediately in both the panel and transfer window.

Settings are stored atomically in
`~/Library/Application Support/HL100Drop/settings.json`; replacing the app keeps
the device ID, preferences, and received files. `HL100_SETTINGS_PATH` overrides
the settings file for isolated tests. An unreadable settings file is reported
instead of silently overwritten during launch.

## Signing and distribution

Default builds are ad-hoc signed for local development, with signatures checked
by the build script. They are **not** Developer ID signed or notarized, so a
downloaded build may be blocked by Gatekeeper. Public distribution requires your
Apple Developer ID Application identity and notary credentials:

```sh
export MACOS_SIGN_IDENTITY='Developer ID Application: Your Name (TEAMID)'
export MACOS_NOTARY_PROFILE='your-keychain-profile'
export BUILD_NUMBER=2
./scripts/build-macos.sh
```

Store the notary profile with `xcrun notarytool store-credentials` beforehand.
The script signs the embedded dylib first, enables the hardened runtime with a
Developer ID identity, signs the application, creates the DMG, submits it to
Apple, staples the ticket, and calculates the final SHA-256. This client is not
App Sandbox enabled: it uses user-selected filesystem paths through the common
Rust engine. It requests local-network access using
`NSLocalNetworkUsageDescription`; it does not use Bonjour service browsing.

## Updates

Only stable GitHub Releases with a newer three-part version and a macOS asset
are offered. Asset name: `HL100Drop-VERSION-universal.dmg`. Publish its final
SHA-256 in `checksums.txt` alongside the Windows/Android checksums, or provide
GitHub's `sha256:` asset digest. Downloads happen only when the user clicks the
update button. A missing/mismatched digest rejects the download. After a
verified download, Finder opens the DMG; finish transfers, quit the old app, and
replace it in Applications. The updater never replaces a running app.

## Validation

`test-macos.sh` runs the Rust regression tests, then compiles a Swift C ABI
smoke test against the embedded dylib. It performs real loopback TCP sends and
receives, checks binary/Unicode/zero-byte files, empty directories and collision
preservation, and checks persistent settings and update metadata parsing.

To render fixture data through the actual AppKit views without starting the
network engine or touching preferences:

```sh
HL100_UI_SMOKE_PATH=/tmp/drop-ui.png \
  'artifacts/macos/HL100 Drop.app/Contents/MacOS/HL100Drop' --ui-smoke --interaction-smoke
```

The interaction smoke test uses real AppKit pasteboards and the production
drop handlers: drag entry opens the panel, icon drops stage files, non-file
content is rejected, Pin and outside-click dismissal rules are checked, and
received-file buttons in both views pass the exact saved URL to the Finder action.
Application deactivation alone does not dismiss an ongoing drag target.
Add `--ui-settings-smoke` to render the settings view or `--ui-desktop-smoke` for the transfer window. These checks require a
logged-in graphical macOS session; actual Finder drag sessions still need a
manual device check.

`./scripts/test-macos.sh --ui` runs both the transfer and panel-interaction
checks and saves the rendered compact panel under `apps/macos/build/tests/`.

Before a stable release, still test the ARM slice on an actual M1 Pro and both
directions with Windows/Android over a physical LAN. Also check local-network
permissions, Finder picker/drag-and-drop, background reception, large-file
cancellation/network loss, macOS 12 compatibility, and a real signed update.
