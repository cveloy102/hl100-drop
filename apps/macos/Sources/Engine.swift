import Foundation

struct DropError: LocalizedError {
    let message: String
    var errorDescription: String? { message }
}

struct Peer: Equatable {
    let id: String
    let name: String
    let address: String
    let port: Int
    let platform: String

    init?(_ value: [String: Any]) {
        guard let id = value["id"] as? String, let name = value["name"] as? String,
              let address = value["address"] as? String, let port = value["port"] as? Int,
              (1...65535).contains(port) else { return nil }
        self.id = id; self.name = name; self.address = address; self.port = port
        platform = value["platform"] as? String ?? "unknown"
    }
}

// Every C ABI call, including manifest collection, runs off the AppKit thread.
final class Engine {
    private let queue = DispatchQueue(label: "ru.hl100.drop.engine", qos: .userInitiated)

    static func call(_ command: [String: Any]) throws -> [String: Any] {
        let data = try JSONSerialization.data(withJSONObject: command)
        guard let string = String(data: data, encoding: .utf8) else {
            throw DropError(message: "Не удалось подготовить команду")
        }
        let pointer = string.withCString { hl100_command($0) }
        guard let pointer = pointer else { throw DropError(message: "Ядро не вернуло ответ") }
        defer { hl100_free(pointer) }
        let response = Data(String(cString: pointer).utf8)
        guard let result = try JSONSerialization.jsonObject(with: response) as? [String: Any] else {
            throw DropError(message: "Некорректный ответ ядра")
        }
        if result["ok"] as? Bool == false {
            throw DropError(message: result["error"] as? String ?? "Ошибка ядра")
        }
        return result
    }

    func perform(_ command: [String: Any], completion: @escaping (Result<[String: Any], Error>) -> Void) {
        queue.async {
            let result = Result { try Self.call(command) }
            DispatchQueue.main.async { completion(result) }
        }
    }
}

struct Settings: Codable {
    var id = UUID().uuidString
    var name = Host.current().localizedName ?? "Mac"
    var folder = FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Downloads/HL100 Drop").path
    var configured = false
    var checkUpdates = true

    static var location: URL {
        if let path = ProcessInfo.processInfo.environment["HL100_SETTINGS_PATH"] {
            return URL(fileURLWithPath: path)
        }
        return FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("HL100Drop/settings.json")
    }

    static func load() throws -> Settings {
        guard FileManager.default.fileExists(atPath: location.path) else { return Settings() }
        return try JSONDecoder().decode(Settings.self, from: Data(contentsOf: location))
    }

    func save() throws {
        try FileManager.default.createDirectory(at: Self.location.deletingLastPathComponent(),
                                                withIntermediateDirectories: true)
        try JSONEncoder().encode(self).write(to: Self.location, options: .atomic)
    }

    func validateFolder() throws {
        try FileManager.default.createDirectory(atPath: folder, withIntermediateDirectories: true)
        // Check actual write access before starting automatic reception.
        let probe = URL(fileURLWithPath: folder).appendingPathComponent(".hl100-\(UUID().uuidString)")
        try Data().write(to: probe, options: .withoutOverwriting)
        try FileManager.default.removeItem(at: probe)
    }
}

func bytes(_ value: Double) -> String {
    let units = ["Б", "КБ", "МБ", "ГБ", "ТБ"]
    var number = max(0, value), index = 0
    while number >= 1024 && index < units.count - 1 { number /= 1024; index += 1 }
    return String(format: "%.1f %@", number, units[index])
}
