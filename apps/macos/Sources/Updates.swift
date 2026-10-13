import Foundation
import CryptoKit

struct Release {
    let version: String
    let url: URL
    let digest: String?
    let checksums: URL?
    var filename: String { "HL100Drop-\(version)-universal.dmg" }
}

enum Updates {
    static let repository = "cveloy102/hl100-drop"
    static var current: String { Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "0.1.1" }
    private static let queue = DispatchQueue(label: "ru.hl100.drop.updates", qos: .utility)
    private static let session: URLSession = {
        let config = URLSessionConfiguration.ephemeral
        config.timeoutIntervalForRequest = 30; config.timeoutIntervalForResource = 1200
        config.httpAdditionalHeaders = ["User-Agent": "HL100Drop/\(current)", "Accept": "application/vnd.github+json"]
        return URLSession(configuration: config)
    }()

    static func version(_ text: String) -> [Int]? {
        let pieces = text.split(separator: ".", omittingEmptySubsequences: false)
        guard pieces.count == 3 else { return nil }
        let numbers = pieces.compactMap { Int($0) }
        return numbers.count == 3 && numbers.allSatisfy { $0 >= 0 } ? numbers : nil
    }
    static func validate(_ url: URL) throws {
        guard url.scheme == "https", url.host == "github.com",
              url.path.hasPrefix("/\(repository)/releases/download/"), url.user == nil else {
            throw DropError(message: "Некорректный адрес обновления")
        }
    }
    private static func data(_ url: URL, completion: @escaping (Result<Data, Error>) -> Void) {
        session.dataTask(with: url) { data, response, error in
            if let error = error { completion(.failure(error)); return }
            guard let response = response as? HTTPURLResponse, (200...299).contains(response.statusCode), let data = data else {
                completion(.failure(DropError(message: "Сервер обновлений недоступен"))); return
            }
            completion(.success(data))
        }.resume()
    }
    static func check(completion: @escaping (Result<Release?, Error>) -> Void) {
        let url = URL(string: "https://api.github.com/repos/\(repository)/releases/latest")!
        session.dataTask(with: url) { data, response, error in
            let result = Result<Release?, Error> {
                if let error = error { throw error }
                if (response as? HTTPURLResponse)?.statusCode == 404 { return nil }
                guard let response = response as? HTTPURLResponse, response.statusCode == 200, let data = data,
                      let root = try JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                    throw DropError(message: "Не удалось проверить обновления")
                }
                guard root["prerelease"] as? Bool != true, root["draft"] as? Bool != true,
                      let tag = root["tag_name"] as? String else { return nil }
                let text = tag.hasPrefix("v") ? String(tag.dropFirst()) : tag
                guard let latest = version(text), let installed = version(current), installed.lexicographicallyPrecedes(latest),
                      let assets = root["assets"] as? [[String: Any]],
                      let asset = assets.first(where: { $0["name"] as? String == "HL100Drop-\(text)-universal.dmg" }),
                      let link = asset["browser_download_url"] as? String, let url = URL(string: link) else { return nil }
                try validate(url)
                let checksumText = assets.first(where: { $0["name"] as? String == "checksums.txt" })?["browser_download_url"] as? String
                let checksums = checksumText.flatMap { URL(string: $0) }
                if let checksums = checksums { try validate(checksums) }
                return Release(version: text, url: url, digest: asset["digest"] as? String, checksums: checksums)
            }
            DispatchQueue.main.async { completion(result) }
        }.resume()
    }

    static func checksum(_ text: String, filename: String) -> String? {
        for line in text.split(separator: "\n") {
            let fields = line.split(whereSeparator: { $0.isWhitespace })
            if fields.count == 2, fields[1].trimmingCharacters(in: CharacterSet(charactersIn: "*")) == filename,
               validDigest(String(fields[0])) { return String(fields[0]).lowercased() }
        }
        return nil
    }
    static func validDigest(_ text: String) -> Bool {
        text.count == 64 && text.utf8.allSatisfy { (48...57).contains($0) || (65...70).contains($0) || (97...102).contains($0) }
    }

    static func download(_ release: Release, completion: @escaping (Result<URL, Error>) -> Void) {
        func finish(_ result: Result<URL, Error>) { DispatchQueue.main.async { completion(result) } }
        func fetch(_ digest: String) {
            session.downloadTask(with: release.url) { temporary, response, error in
                if let error = error { finish(.failure(error)); return }
                guard let temporary = temporary, let response = response as? HTTPURLResponse,
                      response.statusCode == 200 else {
                    finish(.failure(DropError(message: "Не удалось загрузить обновление"))); return
                }
                // URLSession removes its temporary file when this callback returns.
                let cache = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
                    .appendingPathComponent("HL100Drop/\(UUID().uuidString)")
                let destination = cache.appendingPathComponent(release.filename)
                do {
                    try FileManager.default.createDirectory(at: cache, withIntermediateDirectories: true)
                    try FileManager.default.moveItem(at: temporary, to: destination)
                } catch { try? FileManager.default.removeItem(at: cache); finish(.failure(error)); return }
                queue.async {
                    let result = Result<URL, Error> {
                        let handle = try FileHandle(forReadingFrom: destination)
                        defer { try? handle.close() }
                        var hash = SHA256()
                        while let chunk = try handle.read(upToCount: 1024 * 1024), !chunk.isEmpty { hash.update(data: chunk) }
                        let actual = hash.finalize().map { String(format: "%02x", $0) }.joined()
                        guard actual == digest.lowercased() else { throw DropError(message: "Контрольная сумма обновления не совпала") }
                        return destination
                    }
                    if case .failure = result { try? FileManager.default.removeItem(at: cache) }
                    finish(result)
                }
            }.resume()
        }
        do { try validate(release.url) } catch { finish(.failure(error)); return }
        if let digest = release.digest, digest.hasPrefix("sha256:"), validDigest(String(digest.dropFirst(7))) {
            fetch(String(digest.dropFirst(7))); return
        }
        guard let checksums = release.checksums else {
            finish(.failure(DropError(message: "В релизе отсутствует SHA-256 обновления"))); return
        }
        do { try validate(checksums) } catch { finish(.failure(error)); return }
        data(checksums) { result in
            do {
                let content = String(decoding: try result.get(), as: UTF8.self)
                guard let digest = checksum(content, filename: release.filename) else {
                    throw DropError(message: "В релизе отсутствует SHA-256 обновления")
                }
                fetch(digest)
            } catch { finish(.failure(error)) }
        }
    }
}
