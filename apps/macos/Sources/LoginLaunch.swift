import AppKit
import CoreServices
import ServiceManagement

enum LoginLaunchState { case disabled, enabled, requiresApproval }
protocol LoginLaunchManaging: AnyObject {
    var state: LoginLaunchState { get }
    func setEnabled(_ enabled: Bool) throws
    func openApprovalSettings()
}

final class LoginLaunch: LoginLaunchManaging {
    static let label = "ru.hl100.drop.login"
    let appURL: URL
    let legacyURL: URL
    private let forceLegacy: Bool
    init(appURL: URL = Bundle.main.bundleURL, legacyURL: URL? = nil, forceLegacy: Bool = false) {
        self.appURL = appURL.standardizedFileURL
        self.legacyURL = legacyURL ?? FileManager.default.homeDirectoryForCurrentUser
            .appendingPathComponent("Library/LaunchAgents/\(Self.label).plist")
        self.forceLegacy = forceLegacy
    }
    private var legacyEnabled: Bool {
        guard let data = try? Data(contentsOf: legacyURL),
              let plist = try? PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any] else { return false }
        return plist["Label"] as? String == Self.label && plist["RunAtLoad"] as? Bool == true
            && plist["ProgramArguments"] as? [String] == Self.arguments(for: appURL)
    }
    var state: LoginLaunchState {
        if #available(macOS 13.0, *), !forceLegacy {
            switch SMAppService.mainApp.status {
            case .enabled: return .enabled
            case .requiresApproval: return .requiresApproval
            default: break
            }
        }
        return legacyEnabled ? .enabled : .disabled
    }
    static func arguments(for appURL: URL) -> [String] {
        // LaunchServices reuses the existing process. No shell quoting or second engine.
        ["/usr/bin/open", "-g", "-a", appURL.path, "--args", "--login"]
    }
    func setEnabled(_ enabled: Bool) throws {
        if enabled {
            guard appURL.pathExtension == "app", FileManager.default.fileExists(atPath: appURL.path),
                  !appURL.path.hasPrefix("/Volumes/"), !appURL.path.contains("/AppTranslocation/") else {
                throw DropError(message: "Сначала перенеси HL100 Drop из образа в папку «Программы» и запусти оттуда.")
            }
        }
        if #available(macOS 13.0, *), !forceLegacy {
            let service = SMAppService.mainApp
            if enabled {
                if service.status != .enabled && service.status != .requiresApproval { try service.register() }
            } else if service.status != .notRegistered { try service.unregister() }
            // Retire a Monterey registration only after the new API succeeds.
            try removeLegacy()
        } else if enabled {
            let plist: [String: Any] = ["Label": Self.label, "ProgramArguments": Self.arguments(for: appURL),
                                        "RunAtLoad": true, "LimitLoadToSessionType": "Aqua", "KeepAlive": false]
            let data = try PropertyListSerialization.data(fromPropertyList: plist, format: .xml, options: 0)
            try FileManager.default.createDirectory(at: legacyURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            try data.write(to: legacyURL, options: .atomic)
        } else { try removeLegacy() }
    }
    private func removeLegacy() throws {
        if FileManager.default.fileExists(atPath: legacyURL.path) { try FileManager.default.removeItem(at: legacyURL) }
    }
    func openApprovalSettings() {
        if #available(macOS 13.0, *) { SMAppService.openSystemSettingsLoginItems() }
    }
    static func isLoginLaunch(arguments: [String], event: NSAppleEventDescriptor?) -> Bool {
        arguments.contains("--login")
            || event?.paramDescriptor(forKeyword: keyAEPropData)?.enumCodeValue == keyAELaunchedAsLogInItem
            || event?.paramDescriptor(forKeyword: keyAELaunchedAsLogInItem)?.booleanValue == true
    }
}

// UI fixtures never register or inspect the user's actual login items.
final class PreviewLoginLaunch: LoginLaunchManaging {
    var state: LoginLaunchState = .disabled
    var failChange = false
    func setEnabled(_ enabled: Bool) throws {
        if failChange { throw DropError(message: "Fixture: registration denied") }
        state = enabled ? .enabled : .disabled
    }
    func openApprovalSettings() {}
}
