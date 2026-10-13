import Foundation
import AppKit
import CoreServices

func require(_ condition: @autoclosure () -> Bool, _ message: String) throws {
    if !condition() { throw DropError(message: message) }
}

let root = FileManager.default.temporaryDirectory.appendingPathComponent("hl100-macos-test-\(UUID().uuidString)")
defer { try? FileManager.default.removeItem(at: root) }
do {
    let fakeApp = root.appendingPathComponent("Программы/HL100 Drop.app")
    try FileManager.default.createDirectory(at: fakeApp, withIntermediateDirectories: true)
    let agent = root.appendingPathComponent("LaunchAgents/test.plist")
    let login = LoginLaunch(appURL: fakeApp, legacyURL: agent, forceLegacy: true)
    try require(login.state == .disabled, "Login should start disabled")
    try login.setEnabled(true)
    let plist = try PropertyListSerialization.propertyList(from: Data(contentsOf: agent), format: nil) as! [String: Any]
    try require(login.state == .enabled && plist["RunAtLoad"] as? Bool == true && plist["KeepAlive"] as? Bool == false, "Login agent did not enable")
    try require(plist["ProgramArguments"] as? [String] == ["/usr/bin/open", "-g", "-a", fakeApp.path, "--args", "--login"], "Unsafe or incorrect login command")
    try require(LoginLaunch(appURL: fakeApp, legacyURL: agent, forceLegacy: true).state == .enabled, "Login state not restored")
    try login.setEnabled(false); try login.setEnabled(false)
    try require(login.state == .disabled && !FileManager.default.fileExists(atPath: agent.path), "Login agent not removed")
    try require(LoginLaunch.isLoginLaunch(arguments: ["Drop", "--login"], event: nil), "Background launch flag missed")
    try require(!LoginLaunch.isLoginLaunch(arguments: ["Drop"], event: nil), "Manual launch misdetected")
    let event = NSAppleEventDescriptor(eventClass: kCoreEventClass, eventID: kAEOpenApplication,
                                       targetDescriptor: nil, returnID: AEReturnID(kAutoGenerateReturnID), transactionID: AETransactionID(kAnyTransactionID))
    event.setParam(NSAppleEventDescriptor(enumCode: keyAELaunchedAsLogInItem), forKeyword: keyAEPropData)
    try require(LoginLaunch.isLoginLaunch(arguments: ["Drop"], event: event), "Native login event missed")
    print("PASS: login enable/disable/reload, Unicode/spaces in app path, background launch flag and Apple event")
    let source = root.appendingPathComponent("source/Папка")
    let receive = root.appendingPathComponent("receive")
    try FileManager.default.createDirectory(at: source.appendingPathComponent("empty"), withIntermediateDirectories: true)
    let payload = Data((0..<(3 * 1024 * 1024 + 17)).map { UInt8($0 % 251) })
    try payload.write(to: source.appendingPathComponent("файл.bin"))
    try Data().write(to: source.appendingPathComponent("zero.txt"))
    var settings = Settings(); settings.name = "Swift ABI test"; settings.folder = receive.path; settings.configured = true
    try settings.validateFolder(); try settings.save()
    let loaded = try Settings.load()
    try require(loaded.id == settings.id && loaded.folder == settings.folder, "Settings did not survive reload")
    try require(Updates.version("1.2.3") == [1, 2, 3] && Updates.version("1.2.3-beta") == nil, "Version parsing failed")
    let digest = String(repeating: "ab", count: 32)
    try require(Updates.checksum("\(digest)  *test.dmg\n", filename: "test.dmg") == digest, "Checksum parsing failed")
    try require(Updates.checksum("\(digest)  other.dmg", filename: "test.dmg") == nil, "Wrong asset checksum accepted")
    try require(!Updates.validDigest(String(repeating: "z", count: 64)), "Invalid digest accepted")
    do { try Updates.validate(URL(string: "https://example.com/evil.dmg")!); throw DropError(message: "Untrusted URL accepted") }
    catch let error as DropError { try require(error.message == "Некорректный адрес обновления", "URL validation failed") }
    _ = try Engine.call(["op": "start", "config": ["id": settings.id, "name": settings.name, "folder": receive.path, "port": 0]])
    defer { _ = try? Engine.call(["op": "stop"]) }
    let initial = try Engine.call(["op": "poll"])
    let port = (initial["events"] as? [[String: Any]])?.first(where: { $0["type"] as? String == "started" })?["port"] as? Int
    guard let port = port else { throw DropError(message: "No listener port") }
    var receivedPaths: Set<String> = []
    for _ in 0..<2 {
        _ = try Engine.call(["op": "send", "address": "127.0.0.1", "port": port, "name": "Loopback", "paths": [source.path]])
        let deadline = Date().addingTimeInterval(20)
        var completed = 0, described = 0, revealed = 0
        while Date() < deadline && completed < 2 {
            let state = try Engine.call(["op": "poll"])
            for event in state["events"] as? [[String: Any]] ?? [] {
                if event["type"] as? String == "transfer" {
                    let entries = event["entries"] as? [[String: Any]] ?? []
                    try require(entries.contains { $0["path"] as? String == "Папка/файл.bin" && $0["size"] as? Int == payload.count }, "File names/sizes missing")
                    let visible = event["display_entries"] as? [[String: Any]] ?? []
                    try require(visible.count == 1 && visible[0]["path"] as? String == "Папка" && visible[0]["directory"] as? Bool == true && visible[0]["size"] as? Int == payload.count, "Folder summary missing")
                    described += 1
                }
                if event["type"] as? String == "file" {
                    try require(event["directory"] as? Bool == true && event["path"] as? String == "Папка", "Expected one finalized folder event")
                    guard let path = event["local_path"] as? String else { throw DropError(message: "No finalized local path") }
                    try require(FileManager.default.fileExists(atPath: path), "Reveal path does not exist")
                    try require(receivedPaths.insert(path).inserted, "Collision returned the previous folder path")
                    let folder = URL(fileURLWithPath: path)
                    let revealedData = try Data(contentsOf: folder.appendingPathComponent("файл.bin"))
                    try require(revealedData == payload, "Revealed folder content mismatch")
                    revealed += 1
                }
                if event["type"] as? String == "complete" {
                    try require(event["status"] as? String == "done", "Transfer failed: \(event)")
                    completed += 1
                }
            }
            Thread.sleep(forTimeInterval: 0.02)
        }
        try require(completed == 2, "Timed out waiting for send and receive completion")
        try require(described == 2 && revealed == 1, "Incomplete file metadata events")
    }
    let folders = try FileManager.default.contentsOfDirectory(at: receive, includingPropertiesForKeys: nil).filter { $0.lastPathComponent != ".drop-state" }
    try require(folders.count == 2, "Collision did not preserve both folders")
    for folder in folders {
        let received = try Data(contentsOf: folder.appendingPathComponent("файл.bin"))
        try require(received == payload, "Binary payload changed")
        try require(FileManager.default.fileExists(atPath: folder.appendingPathComponent("empty").path), "Empty folder lost")
        let zero = try Data(contentsOf: folder.appendingPathComponent("zero.txt"))
        try require(zero.isEmpty, "Zero-byte file changed")
    }
    let final = try Engine.call(["op": "poll"])
    try require(final["busy"] as? Bool == false, "Engine still busy")
    print("PASS: Swift C ABI loopback, Unicode/binary/zero-byte files, empty folders, collisions, persistent settings, update metadata validation")
} catch {
    fputs("FAIL: \(error)\n", stderr)
    exit(1)
}
