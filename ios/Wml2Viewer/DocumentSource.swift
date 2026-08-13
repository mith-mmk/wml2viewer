import CryptoKit
import Foundation
import UniformTypeIdentifiers

protocol DocumentSource: Sendable {
    var sourceID: UUID { get }
    var displayName: String { get }
    var rootURL: URL { get }
    var isFolder: Bool { get }

    func list() async throws -> [PageItem]
    func read(_ item: PageItem) async throws -> Data
}

struct SecurityScopedDocumentSource: DocumentSource, @unchecked Sendable {
    let sourceID: UUID
    let displayName: String
    let rootURL: URL
    let isFolder: Bool

    func list() async throws -> [PageItem] {
        try await withSecurityScope {
            if isFolder {
                return try coordinatedRead(at: rootURL) { coordinatedRoot in
                    let keys: Set<URLResourceKey> = [.isRegularFileKey, .isDirectoryKey, .nameKey]
                    let urls = try FileManager.default.contentsOfDirectory(
                        at: coordinatedRoot,
                        includingPropertiesForKeys: Array(keys),
                        options: [.skipsHiddenFiles]
                    )
                    return urls.compactMap(Self.pageItem(for:)).sorted {
                        $0.displayName.localizedStandardCompare($1.displayName) == .orderedAscending
                    }
                }
            }
            return try coordinatedRead(at: rootURL) { coordinatedURL in
                [PageItem(
                    id: DocumentEntryIdentity.opaqueIdentifier(for: coordinatedURL),
                    url: coordinatedURL,
                    displayName: displayName,
                    isArchive: Self.isArchive(coordinatedURL)
                )]
            }
        }
    }

    func read(_ item: PageItem) async throws -> Data {
        try await withSecurityScope {
            try coordinatedRead(at: item.url) { coordinatedURL in
                try Data(contentsOf: coordinatedURL, options: [.mappedIfSafe])
            }
        }
    }

    private func withSecurityScope<T: Sendable>(_ operation: () throws -> T) async throws -> T {
        let scoped = rootURL.startAccessingSecurityScopedResource()
        defer { if scoped { rootURL.stopAccessingSecurityScopedResource() } }
        return try operation()
    }

    private func coordinatedRead<T>(at url: URL, operation: (URL) throws -> T) throws -> T {
        let coordinator = NSFileCoordinator(filePresenter: nil)
        var coordinationError: NSError?
        var result: Result<T, Error>?
        coordinator.coordinate(
            readingItemAt: url,
            options: .withoutChanges,
            error: &coordinationError
        ) { coordinatedURL in
            result = Result { try operation(coordinatedURL) }
        }
        if let coordinationError { throw coordinationError }
        guard let result else { throw CocoaError(.fileReadUnknown) }
        return try result.get()
    }

    private static func pageItem(for url: URL) -> PageItem? {
        guard let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isRegularFileKey]),
              values.isRegularFile == true else { return nil }
        let item = PageItem(
            id: DocumentEntryIdentity.opaqueIdentifier(for: url),
            url: url,
            displayName: url.lastPathComponent,
            isArchive: isArchive(url)
        )
        return item.isSupported ? item : nil
    }

    private static func isArchive(_ url: URL) -> Bool {
        MobileFileTypePolicy.shared.archiveFormat(for: url.lastPathComponent) != nil
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

    var errorDescription: String? {
        switch self {
        case .unsupportedItem: String(localized: "Unsupported document")
        case .folderRequired: String(localized: "Select the containing folder for this listed file")
        case .selectedFileNotFound: String(localized: "The selected folder does not contain the opened file")
        case .permissionDenied: String(localized: "The document is no longer available")
        }
    }
}
