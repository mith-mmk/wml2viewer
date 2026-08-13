import Foundation

/// On-demand cache: only selected files/entries are materialized; whole folders are never copied.
actor MaterializeCache {
    static let shared = MaterializeCache(limitBytes: automaticLimitBytes())
    private struct Entry {
        let size: Int
        var lastAccess: UInt64
    }

    private var limit: Int
    private let entryLimit: Int
    private var bytes = 0
    private var clock: UInt64 = 0
    private var files: [URL: Entry] = [:]

    init(
        limitBytes: Int = 256 * 1024 * 1024,
        entryLimitBytes: Int = 64 * 1024 * 1024
    ) {
        limit = max(1, limitBytes)
        entryLimit = max(1, entryLimitBytes)
        Self.removeOrphanedFiles()
    }

    func materialize(_ data: Data, suggestedExtension: String) throws -> URL {
        guard data.count <= entryLimit else { throw CocoaError(.fileWriteOutOfSpace) }
        let root = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        let url = root.appendingPathComponent("materialized-\(UUID().uuidString).\(suggestedExtension)")
        try data.write(to: url, options: .atomic)
        clock &+= 1
        files[url] = Entry(size: data.count, lastAccess: clock)
        bytes += data.count
        evictIfNeeded()
        return url
    }

    func touch(_ url: URL) {
        guard var entry = files[url] else { return }
        clock &+= 1
        entry.lastAccess = clock
        files[url] = entry
    }

    func remove(_ url: URL) {
        guard let entry = files.removeValue(forKey: url) else { return }
        bytes -= entry.size
        try? FileManager.default.removeItem(at: url)
    }

    func setTotalLimit(_ newLimit: Int?) {
        limit = max(1, newLimit ?? Self.automaticLimitBytes())
        evictIfNeeded()
    }

    #if DEBUG
    var cachedByteCount: Int { bytes }
    var cachedFileCount: Int { files.count }
    #endif

    private func evictIfNeeded() {
        while bytes > limit,
              let victim = files.min(by: { $0.value.lastAccess < $1.value.lastAccess })?.key {
            remove(victim)
        }
    }

    static func automaticLimitBytes(
        freeBytes: Int64? = nil,
        reserveBytes: Int64 = 1 * 1024 * 1024 * 1024
    ) -> Int {
        let free = freeBytes ?? {
            let root = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            let attributes = try? FileManager.default.attributesOfFileSystem(forPath: root.path)
            return (attributes?[.systemFreeSize] as? NSNumber)?.int64Value ?? 0
        }()
        guard free > 0 else { return 256 * 1024 * 1024 }
        let usable = max(Int64(1), free - reserveBytes)
        let tenPercent = max(Int64(1), free / 10)
        let target = min(max(tenPercent, 256 * 1024 * 1024), 2 * 1024 * 1024 * 1024)
        return Int(min(target, usable))
    }

    private static func removeOrphanedFiles() {
        let root = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        let urls = (try? FileManager.default.contentsOfDirectory(
            at: root,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        )) ?? []
        for url in urls where url.lastPathComponent.hasPrefix("materialized-") {
            try? FileManager.default.removeItem(at: url)
        }
    }
}
