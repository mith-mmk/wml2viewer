import Foundation

/// On-demand cache: only selected files/entries are materialized; whole folders are never copied.
actor MaterializeCache {
    static let shared = MaterializeCache()
    private let limit = 64 * 1024 * 1024
    private var bytes = 0
    private var files: [URL: Int] = [:]

    func materialize(_ data: Data, suggestedExtension: String) throws -> URL {
        guard data.count <= limit else { throw CocoaError(.fileWriteOutOfSpace) }
        let root = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
        let url = root.appendingPathComponent("materialized-\(UUID().uuidString).\(suggestedExtension)")
        try data.write(to: url, options: .atomic)
        files[url] = data.count
        bytes += data.count
        evictIfNeeded()
        return url
    }

    private func evictIfNeeded() {
        while bytes > limit, let victim = files.keys.first {
            bytes -= files.removeValue(forKey: victim) ?? 0
            try? FileManager.default.removeItem(at: victim)
        }
    }
}
