import Foundation

actor BookmarkStore {
    private let fileURL: URL
    private var records: [BookmarkRecord] = []

    init(fileManager: FileManager = .default) {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        fileURL = appSupport.appendingPathComponent("bookmarks-v1.json")
    }

    func load() throws -> [BookmarkRecord] {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return [] }
        records = try JSONDecoder().decode([BookmarkRecord].self, from: Data(contentsOf: fileURL))
        return records
    }

    func replace(_ records: [BookmarkRecord]) throws {
        self.records = records
        let directory = fileURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let data = try JSONEncoder().encode(records)
        let temporary = directory.appendingPathComponent("bookmarks-v1.json.tmp")
        try data.write(to: temporary, options: [.atomic])
        if FileManager.default.fileExists(atPath: fileURL.path) {
            _ = try FileManager.default.replaceItemAt(fileURL, withItemAt: temporary)
        } else {
            try FileManager.default.moveItem(at: temporary, to: fileURL)
        }
    }

    func upsert(_ record: BookmarkRecord) throws {
        var next = records
        next.removeAll { $0.sourceID == record.sourceID }
        next.insert(record, at: 0)
        try replace(next)
    }

    func remove(sourceID: UUID) throws {
        try replace(records.filter { $0.sourceID != sourceID })
    }
}

actor ConfigStore {
    private let fileURL: URL
    private var latestSaveSequence: UInt64 = 0

    init(fileManager: FileManager = .default) {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        fileURL = appSupport.appendingPathComponent("mobile-config-v1.json")
    }

    init(fileURL: URL) {
        self.fileURL = fileURL
    }

    func load() -> MobileConfigV1 {
        guard let data = try? Data(contentsOf: fileURL),
              let config = try? JSONDecoder().decode(MobileConfigV1.self, from: data) else {
            return MobileConfigV1()
        }
        return config
    }

    func save(_ config: MobileConfigV1, sequence: UInt64) throws {
        guard sequence >= latestSaveSequence else { return }
        let directory = fileURL.deletingLastPathComponent()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let data = try JSONEncoder().encode(config)
        let temporary = directory.appendingPathComponent("mobile-config-v1.json.tmp")
        try data.write(to: temporary, options: [.atomic])
        if FileManager.default.fileExists(atPath: fileURL.path) {
            _ = try FileManager.default.replaceItemAt(fileURL, withItemAt: temporary)
        } else {
            try FileManager.default.moveItem(at: temporary, to: fileURL)
        }
        latestSaveSequence = sequence
    }
}
