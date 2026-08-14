import CryptoKit
import FileProvider
import Foundation

struct ProviderOpaqueIdentity: Equatable, Sendable {
    let domainID: String
    let itemID: String

    static func resolve(for url: URL, bookmark: Data) async -> ProviderOpaqueIdentity {
        if let provider = await providerIdentity(for: url) {
            return provider
        }
        return ProviderOpaqueIdentity(
            domainID: digest(Data("bookmark-fallback".utf8)),
            itemID: digest(bookmark)
        )
    }

    private static func providerIdentity(for url: URL) async -> ProviderOpaqueIdentity? {
        await withCheckedContinuation { continuation in
            NSFileProviderManager.getIdentifierForUserVisibleFile(at: url) {
                itemIdentifier, domainIdentifier, _ in
                guard let itemIdentifier, let domainIdentifier else {
                    continuation.resume(returning: nil)
                    return
                }
                continuation.resume(returning: ProviderOpaqueIdentity(
                    domainID: digest(Data(String(describing: domainIdentifier).utf8)),
                    itemID: digest(Data(String(describing: itemIdentifier).utf8))
                ))
            }
        }
    }

    private static func digest(_ data: Data) -> String {
        SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
    }
}

actor BookmarkStore {
    private let fileURL: URL
    private var records: [BookmarkRecord] = []
    private var isLoaded = false

    init(fileManager: FileManager = .default) {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        fileURL = appSupport.appendingPathComponent("bookmarks-v1.json")
    }

    init(fileURL: URL) {
        self.fileURL = fileURL
    }

    func load() throws -> [BookmarkRecord] {
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            records = []
            isLoaded = true
            return []
        }
        records = try JSONDecoder().decode([BookmarkRecord].self, from: Data(contentsOf: fileURL))
        isLoaded = true
        return records
    }

    func registeredSources() throws -> [RegisteredSourceSummary] {
        try ensureLoaded()
        return records
            .filter(\.isRegistered)
            .sorted {
                ($0.lastOpenedAt ?? $0.registeredAt ?? .distantPast) >
                    ($1.lastOpenedAt ?? $1.registeredAt ?? .distantPast)
            }
            .map(RegisteredSourceSummary.init(record:))
    }

    func record(sourceID: UUID) throws -> BookmarkRecord? {
        try ensureLoaded()
        return records.first { $0.sourceID == sourceID }
    }

    func registeredRecord(
        providerDomainOpaqueID: String,
        providerItemOpaqueID: String
    ) throws -> BookmarkRecord? {
        try ensureLoaded()
        return records.first {
            $0.isRegistered &&
                $0.providerDomainOpaqueID == providerDomainOpaqueID &&
                $0.providerItemOpaqueID == providerItemOpaqueID
        }
    }

    func replace(_ records: [BookmarkRecord]) throws {
        self.records = records
        isLoaded = true
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

    @discardableResult
    func upsert(_ record: BookmarkRecord) throws -> BookmarkRecord {
        try ensureLoaded()
        var next = records
        next.removeAll { $0.sourceID == record.sourceID }
        if record.isRegistered,
           let domain = record.providerDomainOpaqueID,
           let item = record.providerItemOpaqueID {
            next.removeAll {
                $0.isRegistered &&
                    $0.providerDomainOpaqueID == domain &&
                    $0.providerItemOpaqueID == item
            }
        }
        if !record.isRegistered, record.sourceKind == .transientFile {
            next.removeAll { !$0.isRegistered && $0.sourceKind == .transientFile }
        }
        next.insert(record, at: 0)
        try replace(next)
        return record
    }

    func remove(sourceID: UUID) throws {
        try ensureLoaded()
        try replace(records.filter { $0.sourceID != sourceID })
    }

    func updateStatus(sourceID: UUID, status: RegisteredSourceStatus) throws {
        try ensureLoaded()
        guard let index = records.firstIndex(where: { $0.sourceID == sourceID }) else { return }
        records[index].lastKnownStatus = status
        try replace(records)
    }

    func clear() throws {
        try replace([])
    }

    private func ensureLoaded() throws {
        if !isLoaded { _ = try load() }
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
