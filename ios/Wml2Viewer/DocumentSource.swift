import CryptoKit
import CoreGraphics
import Foundation
import ImageIO
import UniformTypeIdentifiers

protocol DocumentSource: Sendable {
    var sourceID: UUID { get }
    var displayName: String { get }
    var rootURL: URL { get }
    var isFolder: Bool { get }

    func snapshot() async throws -> SourceSnapshot
    func read(_ item: PageItem) async throws -> Data
    func stat(_ item: PageItem) async throws -> DocumentEntryStat
    func coordinatedRead(_ item: PageItem) async throws -> Data
    func materialize(_ item: PageItem) async throws -> URL
    func thumbnail(_ item: PageItem, maximumPixelSize: Int) async throws -> CGImage?
    func refresh() async throws -> SourceSnapshot
}

struct DocumentEntryStat: Sendable, Equatable {
    let displayName: String
    let isDirectory: Bool?
    let isRegularFile: Bool?
    let byteSize: Int64?
    let contentTypeIdentifier: String?
}

struct SourceSnapshot: Sendable, Equatable {
    let entries: [PageItem]
    let enumeratedItemCount: Int

    var supportedItemCount: Int { entries.count }
}

/// Cooperative cancellation shared with `NSFileCoordinator`. Cancelling a
/// source opening must return control to the viewer even when a File Provider
/// is still resolving a large directory or a remote placeholder.
final class DocumentSourceCancellation: @unchecked Sendable {
    private let lock = NSLock()
    private var cancelled = false
    private var coordinator: NSFileCoordinator?

    var isCancelled: Bool {
        lock.lock()
        defer { lock.unlock() }
        return cancelled
    }

    func cancel() {
        lock.lock()
        cancelled = true
        let coordinator = coordinator
        lock.unlock()
        coordinator?.cancel()
    }

    func checkCancellation() throws {
        if isCancelled { throw CancellationError() }
    }

    fileprivate func register(_ coordinator: NSFileCoordinator) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard !cancelled else { return false }
        self.coordinator = coordinator
        return true
    }

    fileprivate func unregister() {
        lock.lock()
        coordinator = nil
        lock.unlock()
    }
}

extension DocumentSource {
    func list() async throws -> [PageItem] {
        try await snapshot().entries
    }

    func refresh() async throws -> SourceSnapshot {
        try await snapshot()
    }
}

struct SecurityScopedDocumentSource: DocumentSource, @unchecked Sendable {
    let sourceID: UUID
    let displayName: String
    let rootURL: URL
    let isFolder: Bool

    func snapshot() async throws -> SourceSnapshot {
        try await snapshot(cancellation: nil)
    }

    func snapshot(
        cancellation: DocumentSourceCancellation?,
        progress: (@Sendable (_ processed: Int, _ total: Int) -> Void)? = nil
    ) async throws -> SourceSnapshot {
        try cancellation?.checkCancellation()
        return try await withSecurityScope {
            if isFolder {
                return try await coordinatedRead(
                    at: rootURL,
                    cancellation: cancellation
                ) { coordinatedRoot in
                    try cancellation?.checkCancellation()
                    let keys: Set<URLResourceKey> = [
                        .isRegularFileKey, .isDirectoryKey, .nameKey, .contentTypeKey,
                    ]
                    let urls = try FileManager.default.contentsOfDirectory(
                        at: coordinatedRoot,
                        includingPropertiesForKeys: Array(keys),
                        options: [.skipsHiddenFiles]
                    )
                    progress?(0, urls.count)
                    var entries: [PageItem] = []
                    entries.reserveCapacity(urls.count)
                    for (index, url) in urls.enumerated() {
                        try cancellation?.checkCancellation()
                        if let item = Self.pageItem(for: url) { entries.append(item) }
                        if index == urls.count - 1 || (index + 1).isMultiple(of: 32) {
                            progress?(index + 1, urls.count)
                        }
                    }
                    try cancellation?.checkCancellation()
                    entries.sort {
                        $0.displayName.localizedStandardCompare($1.displayName) == .orderedAscending
                    }
                    try cancellation?.checkCancellation()
                    return SourceSnapshot(
                        entries: entries,
                        enumeratedItemCount: urls.count
                    )
                }
            }
            return try await coordinatedRead(
                at: rootURL,
                cancellation: cancellation
            ) { _ in
                try cancellation?.checkCancellation()
                // `NSFileAccessIntent.url` may be a provider-owned temporary
                // materialization. It is valid only for this coordination
                // block, so retain the original security-scoped root URL for
                // all later reads and bookmark restoration.
                return SourceSnapshot(
                    entries: [PageItem(
                        id: DocumentEntryIdentity.opaqueIdentifier(for: rootURL),
                        url: rootURL,
                        displayName: displayName,
                        isArchive: Self.isArchive(rootURL)
                    )],
                    enumeratedItemCount: 1
                )
            }
        }
    }

    func read(_ item: PageItem) async throws -> Data {
        try await withSecurityScope {
            try await coordinatedRead(at: item.url) { coordinatedURL in
                try Data(contentsOf: coordinatedURL, options: [.mappedIfSafe])
            }
        }
    }

    func stat(_ item: PageItem) async throws -> DocumentEntryStat {
        try await withSecurityScope {
            try await coordinatedRead(at: item.url) { coordinatedURL in
                let values = try coordinatedURL.resourceValues(forKeys: [
                    .nameKey, .isDirectoryKey, .isRegularFileKey, .fileSizeKey, .contentTypeKey,
                ])
                return DocumentEntryStat(
                    displayName: values.name ?? item.displayName,
                    isDirectory: values.isDirectory,
                    isRegularFile: values.isRegularFile,
                    byteSize: values.fileSize.map(Int64.init),
                    contentTypeIdentifier: values.contentType?.identifier
                )
            }
        }
    }

    func coordinatedRead(_ item: PageItem) async throws -> Data {
        try await read(item)
    }

    func materialize(_ item: PageItem) async throws -> URL {
        let data = try await coordinatedRead(item)
        return try await MaterializeCache.shared.materialize(
            data,
            suggestedExtension: item.url.pathExtension.lowercased()
        )
    }

    func thumbnail(_ item: PageItem, maximumPixelSize: Int) async throws -> CGImage? {
        let data = try await coordinatedRead(item)
        guard let imageSource = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceThumbnailMaxPixelSize: max(1, maximumPixelSize),
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: false,
        ]
        return CGImageSourceCreateThumbnailAtIndex(imageSource, 0, options as CFDictionary)
    }

    private func withSecurityScope<T: Sendable>(
        _ operation: () async throws -> T
    ) async throws -> T {
        let scoped = rootURL.startAccessingSecurityScopedResource()
        defer { if scoped { rootURL.stopAccessingSecurityScopedResource() } }
        return try await operation()
    }

    private func coordinatedRead<T: Sendable>(
        at url: URL,
        cancellation: DocumentSourceCancellation? = nil,
        operation: @escaping @Sendable (URL) throws -> T
    ) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            let coordinator = NSFileCoordinator(filePresenter: nil)
            guard cancellation?.register(coordinator) != false else {
                continuation.resume(throwing: CancellationError())
                return
            }
            let intent = NSFileAccessIntent.readingIntent(with: url, options: .withoutChanges)
            let queue = OperationQueue()
            queue.qualityOfService = .userInitiated
            coordinator.coordinate(with: [intent], queue: queue) { error in
                cancellation?.unregister()
                if cancellation?.isCancelled == true {
                    continuation.resume(throwing: CancellationError())
                    return
                }
                if let error {
                    continuation.resume(throwing: error)
                    return
                }
                continuation.resume(with: Result { try operation(intent.url) })
            }
        }
    }

    private static func pageItem(for url: URL) -> PageItem? {
        let values = try? url.resourceValues(forKeys: [
            .isDirectoryKey, .isRegularFileKey, .nameKey, .contentTypeKey,
        ])
        let name = values?.name ?? url.lastPathComponent
        guard DirectoryEntryPolicy.includes(
            name: name,
            isDirectory: values?.isDirectory,
            isRegularFile: values?.isRegularFile,
            declaredMime: values?.contentType?.preferredMIMEType
        ) else { return nil }
        let item = PageItem(
            id: DocumentEntryIdentity.opaqueIdentifier(for: url),
            url: url,
            displayName: name,
            isArchive: isArchive(url)
        )
        return item
    }

    private static func isArchive(_ url: URL) -> Bool {
        MobileFileTypePolicy.shared.archiveFormat(for: url.lastPathComponent) != nil
    }
}

enum DirectoryEntryPolicy {
    /// File Provider placeholders frequently omit `isRegularFile` until their
    /// content is downloaded. Only a positive directory result is exclusionary;
    /// supported placeholder names must remain navigable and hydrate on read.
    static func includes(
        name: String,
        isDirectory: Bool?,
        isRegularFile _: Bool?,
        declaredMime: String?
    ) -> Bool {
        guard isDirectory != true else { return false }
        return MobileFileTypePolicy.shared.isSupported(name, declaredMime: declaredMime)
    }
}

enum DocumentEntryIdentity {
    /// Produces a UI-safe stable identifier without exposing a provider URL or
    /// filesystem path. Providers that do not vend an identifier fall back to a
    /// digest of the direct child's name, which is unique within one directory.
    static func opaqueIdentifier(for url: URL) -> String {
        let identity: String
        if let resourceIdentifier = try? url.resourceValues(
            forKeys: [.fileResourceIdentifierKey]
        ).fileResourceIdentifier {
            identity = "provider:\(String(reflecting: resourceIdentifier))"
        } else {
            identity = "name:\(url.lastPathComponent)"
        }
        return SHA256.hash(data: Data(identity.utf8)).map { String(format: "%02x", $0) }.joined()
    }
}

enum DocumentEntryMatcher {
    static func index(
        selectedOpaqueEntryID: String?,
        selectedFileName: String,
        in items: [PageItem]
    ) -> Int? {
        if let selectedOpaqueEntryID,
           let exact = items.firstIndex(where: { $0.id == selectedOpaqueEntryID }) {
            return exact
        }
        return items.firstIndex {
            $0.displayName.compare(selectedFileName, options: [.caseInsensitive, .widthInsensitive]) == .orderedSame
        }
    }
}

enum DocumentSourceError: LocalizedError {
    case unsupportedItem
    case osAnimationUnsupported
    case unsupportedFileType(String)
    case unreadableDocument(String)
    case folderRequired
    case selectedFileNotFound
    case permissionDenied
    case noSupportedItems
    case noOtherSupportedItems
    case invalidListedFile
    case listedEntryOutsideFolder

    var errorDescription: String? {
        switch self {
        case .unsupportedItem: String(localized: "Unsupported document")
        case .osAnimationUnsupported: String(localized: "This animated image is not supported by the selected OS codec")
        case .unsupportedFileType(let fileExtension):
            fileExtension.isEmpty
                ? String(localized: "Unsupported file type. Choose an image, ZIP/LHA/LZH archive, or WMLTXT file.")
                : String(
                    format: String(localized: "Unsupported file type “.%@”. Choose an image, ZIP/LHA/LZH archive, or WMLTXT file."),
                    fileExtension.uppercased()
                )
        case .unreadableDocument(let displayName):
            String(
                format: String(localized: "Could not display “%@”. The file may be damaged or use an unsupported variant."),
                displayName
            )
        case .folderRequired: String(localized: "Select the containing folder for this listed file")
        case .selectedFileNotFound: String(localized: "The selected folder does not contain the opened file")
        case .permissionDenied: String(localized: "The document is no longer available")
        case .noSupportedItems: String(localized: "No supported files were found in the selected folder")
        case .noOtherSupportedItems: String(localized: "No other supported files were found in the selected folder")
        case .invalidListedFile: String(localized: "The listed file is invalid")
        case .listedEntryOutsideFolder: String(localized: "A listed file entry is outside the selected folder")
        }
    }
}

/// Parses the platform-neutral WML listed-file manifest while keeping every
/// resolved entry inside the user-approved folder. The same normalization
/// rules as the Rust core are applied before any provider URL is read.
enum WmltxtEntryResolver {
    private static let header = "#!WMLViewer2 ListedFile"
    private static let maximumManifestBytes = 64 * 1024 * 1024
    private static let maximumEntries = 20_000

    static func paths(from data: Data) throws -> [String] {
        guard data.count <= maximumManifestBytes else {
            throw DocumentSourceError.invalidListedFile
        }
        guard let text = String(data: data, encoding: .utf8) else {
            throw DocumentSourceError.invalidListedFile
        }
        var lines = text.drop { $0 == "\u{feff}" }.split(whereSeparator: \.isNewline)
        guard let first = lines.first?.trimmingCharacters(in: .whitespaces),
              first.hasPrefix(header) else {
            throw DocumentSourceError.invalidListedFile
        }
        lines.removeFirst()
        var paths: [String] = []
        paths.reserveCapacity(min(lines.count, maximumEntries))
        for rawLine in lines {
            let line = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !line.isEmpty, !line.hasPrefix("#"), !line.hasPrefix("@") else { continue }
            guard paths.count < maximumEntries else { throw DocumentSourceError.invalidListedFile }
            paths.append(try normalize(line))
        }
        return paths
    }

    static func resolve(_ rawPath: String, under root: URL) throws -> URL {
        let relative = try normalize(rawPath)
        let canonicalRoot = root.standardizedFileURL.resolvingSymlinksInPath()
        let candidate = canonicalRoot
            .appendingPathComponent(relative, isDirectory: false)
            .standardizedFileURL
            .resolvingSymlinksInPath()
        let rootPath = canonicalRoot.path.hasSuffix("/") ? canonicalRoot.path : canonicalRoot.path + "/"
        guard candidate.path != canonicalRoot.path, candidate.path.hasPrefix(rootPath) else {
            throw DocumentSourceError.listedEntryOutsideFolder
        }
        return candidate
    }

    private static func normalize(_ rawPath: String) throws -> String {
        let replaced = rawPath.trimmingCharacters(in: .whitespacesAndNewlines).replacingOccurrences(of: "\\", with: "/")
        guard !replaced.isEmpty, !replaced.contains("\0"),
              !replaced.hasPrefix("/"), !replaced.hasPrefix("//") else {
            throw DocumentSourceError.listedEntryOutsideFolder
        }
        let components = replaced.split(separator: "/", omittingEmptySubsequences: true)
        guard !components.isEmpty,
              !components.contains(where: { $0 == ".." || $0.contains(":") }) else {
            throw DocumentSourceError.listedEntryOutsideFolder
        }
        let normalized = components.filter { $0 != "." }.joined(separator: "/")
        guard !normalized.isEmpty else { throw DocumentSourceError.invalidListedFile }
        return normalized
    }
}
