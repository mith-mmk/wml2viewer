import CryptoKit
import Foundation
import UniformTypeIdentifiers

protocol DocumentSource: Sendable {
    var sourceID: UUID { get }
    var displayName: String { get }
    var rootURL: URL { get }
    var isFolder: Bool { get }

    func snapshot() async throws -> SourceSnapshot
    func read(_ item: PageItem) async throws -> Data
}

struct SourceSnapshot: Sendable, Equatable {
    let entries: [PageItem]
    let enumeratedItemCount: Int

    var supportedItemCount: Int { entries.count }
}

extension DocumentSource {
    func list() async throws -> [PageItem] {
        try await snapshot().entries
    }
}

struct SecurityScopedDocumentSource: DocumentSource, @unchecked Sendable {
    let sourceID: UUID
    let displayName: String
    let rootURL: URL
    let isFolder: Bool

    func snapshot() async throws -> SourceSnapshot {
        try await withSecurityScope {
            if isFolder {
                return try await coordinatedRead(at: rootURL) { coordinatedRoot in
                    let keys: Set<URLResourceKey> = [
                        .isRegularFileKey, .isDirectoryKey, .nameKey, .contentTypeKey,
                    ]
                    let urls = try FileManager.default.contentsOfDirectory(
                        at: coordinatedRoot,
                        includingPropertiesForKeys: Array(keys),
                        options: [.skipsHiddenFiles]
                    )
                    let entries = urls.compactMap(Self.pageItem(for:)).sorted {
                        $0.displayName.localizedStandardCompare($1.displayName) == .orderedAscending
                    }
                    return SourceSnapshot(
                        entries: entries,
                        enumeratedItemCount: urls.count
                    )
                }
            }
            return try await coordinatedRead(at: rootURL) { coordinatedURL in
                SourceSnapshot(
                    entries: [PageItem(
                        id: DocumentEntryIdentity.opaqueIdentifier(for: coordinatedURL),
                        url: coordinatedURL,
                        displayName: displayName,
                        isArchive: Self.isArchive(coordinatedURL)
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

    private func withSecurityScope<T: Sendable>(
        _ operation: () async throws -> T
    ) async throws -> T {
        let scoped = rootURL.startAccessingSecurityScopedResource()
        defer { if scoped { rootURL.stopAccessingSecurityScopedResource() } }
        return try await operation()
    }

    private func coordinatedRead<T: Sendable>(
        at url: URL,
        operation: @escaping @Sendable (URL) throws -> T
    ) async throws -> T {
        try await withCheckedThrowingContinuation { continuation in
            let coordinator = NSFileCoordinator(filePresenter: nil)
            let intent = NSFileAccessIntent.readingIntent(with: url, options: .withoutChanges)
            let queue = OperationQueue()
            queue.qualityOfService = .userInitiated
            coordinator.coordinate(with: [intent], queue: queue) { error in
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

    static func paths(from data: Data) throws -> [String] {
        guard let text = String(data: data, encoding: .utf8) else {
            throw DocumentSourceError.invalidListedFile
        }
        var lines = text.drop { $0 == "\u{feff}" }.split(whereSeparator: \.isNewline)
        guard let first = lines.first?.trimmingCharacters(in: .whitespaces),
              first.hasPrefix(header) else {
            throw DocumentSourceError.invalidListedFile
        }
        lines.removeFirst()
        return try lines.compactMap { rawLine in
            let line = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !line.isEmpty, !line.hasPrefix("#"), !line.hasPrefix("@") else { return nil }
            return try normalize(line)
        }
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
