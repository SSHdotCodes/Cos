import Foundation

public actor ThreadStore {
    private let directory: URL
    private let encoder: JSONEncoder
    private let decoder: JSONDecoder

    public init(directory: URL? = nil) {
        let base = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
        self.directory = directory ?? base.appendingPathComponent("Cos/Threads", isDirectory: true)
        self.encoder = JSONEncoder()
        self.decoder = JSONDecoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        encoder.dateEncodingStrategy = .iso8601
        decoder.dateDecodingStrategy = .iso8601
    }

    public func loadAll() throws -> [CosThread] {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let urls = try FileManager.default.contentsOfDirectory(
            at: directory,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        ).filter { $0.pathExtension == "json" }
        return try urls.compactMap { url in
            try decoder.decode(CosThread.self, from: Data(contentsOf: url))
        }.sorted { $0.updatedAt > $1.updatedAt }
    }

    public func save(_ thread: CosThread) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let target = directory.appendingPathComponent("\(thread.id.uuidString).json")
        let temporary = target.appendingPathExtension("tmp")
        try encoder.encode(thread).write(to: temporary, options: [.atomic])
        _ = try FileManager.default.replaceItemAt(target, withItemAt: temporary, backupItemName: nil, options: [])
    }

    public func upsert(_ thread: CosThread) throws {
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let target = directory.appendingPathComponent("\(thread.id.uuidString).json")
        try encoder.encode(thread).write(to: target, options: [.atomic])
    }

    public func delete(id: UUID) throws {
        let target = directory.appendingPathComponent("\(id.uuidString).json")
        if FileManager.default.fileExists(atPath: target.path) {
            try FileManager.default.removeItem(at: target)
        }
    }
}
