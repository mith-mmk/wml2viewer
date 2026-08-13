import Foundation
import SwiftUI

struct EntryRef: Hashable, Codable, Sendable {
    let sourceID: UUID
    let opaqueEntryID: String
}

enum PickerRequest: Hashable {
    case openTarget
    case containingFolder
    case manageFiles

    var acceptsFolders: Bool {
        switch self {
        case .openTarget, .containingFolder: true
        case .manageFiles: false
        }
    }

    func selectionIsFolder(resourceIsDirectory: Bool) -> Bool {
        self == .containingFolder || resourceIsDirectory
    }
}

struct PickerPresentation: Identifiable, Hashable {
    let id: UUID
    let flowID: UUID
    let request: PickerRequest
    let initialDirectoryURL: URL?
}

enum FilesOpenFlowPhase: Equatable {
    case idle
    case presenting(PickerRequest)
    case processing(PickerRequest)

    var blocksViewerInput: Bool { self != .idle }
}

enum SourceConnectionState: Equatable {
    case empty
    case singleFile
    case folder(enumerated: Int, supported: Int)
    case archive(entries: Int)
    case retryableError
}

struct SourceOpeningProgress: Equatable {
    let isFolder: Bool
    var processedItemCount: Int
    var totalItemCount: Int?

    var title: String {
        isFolder ? String(localized: "Scanning folder…") : String(localized: "Opening file…")
    }

    var detail: String {
        guard let totalItemCount, totalItemCount > 0 else {
            return isFolder
                ? String(localized: "Reading the folder contents. This may take a while for cloud or network folders.")
                : String(localized: "Reading the selected file.")
        }
        return String(
            format: String(localized: "Checking files: %lld of %lld"),
            Int64(processedItemCount), Int64(totalItemCount)
        )
    }
}

enum SelectedDocumentPolicy {
    static func declaredMime(for url: URL) -> String? {
        try? url.resourceValues(forKeys: [.contentTypeKey])
            .contentType?.preferredMIMEType
    }

    static func isSupported(url: URL) -> Bool {
        MobileFileTypePolicy.shared.isSupported(
            url.lastPathComponent,
            declaredMime: declaredMime(for: url)
        )
    }

    static func validate(
        name: String,
        isFolder: Bool,
        declaredMime: String? = nil
    ) throws {
        guard !isFolder else { return }
        guard MobileFileTypePolicy.shared.isSupported(name, declaredMime: declaredMime) else {
            throw DocumentSourceError.unsupportedFileType(
                URL(fileURLWithPath: name).pathExtension
            )
        }
    }

    /// File Providers may return an extensionless item whose UTType is known
    /// only through resource values. Use that declaration while the picker
    /// security scope is still alive instead of rejecting a valid image by
    /// filename alone.
    static func validate(url: URL, isFolder: Bool) throws {
        try validate(
            name: url.lastPathComponent,
            isFolder: isFolder,
            declaredMime: declaredMime(for: url)
        )
    }
}

/// Serializes UIKit picker completion, SwiftUI dismissal, and a possible
/// containing-folder follow-up. Every presentation has a unique token, so a
/// delayed File Provider callback cannot complete a newer flow.
struct FilesOpenFlowMachine {
    private(set) var flowID: UUID?
    private(set) var phase: FilesOpenFlowPhase = .idle
    private(set) var activePresentationID: UUID?
    private(set) var activePresentationWasDismissed = false
    private var queuedRequest: (PickerRequest, URL?)?

    mutating func begin(
        _ request: PickerRequest,
        initialDirectoryURL: URL? = nil,
        flowID requestedFlowID: UUID? = nil
    ) -> PickerPresentation? {
        guard phase == .idle else { return nil }
        let flowID = requestedFlowID ?? UUID()
        self.flowID = flowID
        return present(request, initialDirectoryURL: initialDirectoryURL, flowID: flowID)
    }

    mutating func beginResultProcessing(_ presentation: PickerPresentation) -> Bool {
        guard presentation.flowID == flowID,
              presentation.id == activePresentationID,
              case .presenting(let request) = phase,
              request == presentation.request else { return false }
        phase = .processing(request)
        return true
    }

    mutating func queueFollowUp(
        _ request: PickerRequest,
        initialDirectoryURL: URL?
    ) -> PickerPresentation? {
        // Only the primary result may schedule one follow-up. Rejecting a
        // duplicate here prevents a delayed decode/provider callback from
        // presenting a third picker after the folder picker is dismissed.
        guard let flowID,
              case .processing(let activeRequest) = phase,
              activeRequest != .containingFolder,
              queuedRequest == nil else { return nil }
        if activePresentationWasDismissed {
            return present(request, initialDirectoryURL: initialDirectoryURL, flowID: flowID)
        }
        queuedRequest = (request, initialDirectoryURL)
        return nil
    }

    mutating func didDismiss(_ presentationID: UUID) -> PickerPresentation? {
        guard presentationID == activePresentationID else { return nil }
        activePresentationWasDismissed = true
        guard let flowID, let queuedRequest else { return nil }
        self.queuedRequest = nil
        return present(
            queuedRequest.0,
            initialDirectoryURL: queuedRequest.1,
            flowID: flowID
        )
    }

    mutating func finish(flowID: UUID) {
        guard self.flowID == flowID else { return }
        self = FilesOpenFlowMachine()
    }

    private mutating func present(
        _ request: PickerRequest,
        initialDirectoryURL: URL?,
        flowID: UUID
    ) -> PickerPresentation {
        let presentation = PickerPresentation(
            id: UUID(), flowID: flowID, request: request,
            initialDirectoryURL: initialDirectoryURL
        )
        phase = .presenting(request)
        activePresentationID = presentation.id
        activePresentationWasDismissed = false
        return presentation
    }
}

enum ContainingFolderAuthorizationPolicy {
    static func shouldRequest(
        isFolder: Bool,
        isSupported: Bool,
        isSelfContainedArchive: Bool
    ) -> Bool {
        !isFolder && isSupported && !isSelfContainedArchive
    }
}

enum PickerFolderGuidance {
    static func message(for presentation: PickerPresentation) -> String? {
        guard presentation.request == .containingFolder else { return nil }
        return presentation.initialDirectoryURL == nil
            ? String(localized: "Navigate to the folder you want to browse, then tap Open.")
            : String(localized: "To continue from the selected file, keep its containing folder open and tap Open.")
    }
}

enum DisplayFit: String, Codable, CaseIterable {
    case contain, width, height, original
}

/// Runtime double-tap override. It never mutates the persisted initial fit.
enum FitOverridePolicy {
    static func next(current: DisplayFit) -> DisplayFit {
        current == .original ? .contain : .original
    }
}

enum ExternalPageReconciler {
    static func index(oldIndex: Int, oldID: String?, refreshedIDs: [String]) -> Int? {
        guard !refreshedIDs.isEmpty else { return nil }
        if let oldID, let retained = refreshedIDs.firstIndex(of: oldID) { return retained }
        return min(max(oldIndex, 0), refreshedIDs.count - 1)
    }
}

enum FolderTraversalDirection {
    case forward
    case backward
}

/// Resolves a failed/previously failed folder entry without changing the
/// source order. Search continues in the direction requested by the user; at
/// an edge it falls back toward the page they came from instead of wrapping
/// to the opposite end of the folder.
enum FolderPageFailureNavigator {
    static func replacementIndex(
        failedIndex: Int,
        pageCount: Int,
        failedIndices: Set<Int>,
        direction: FolderTraversalDirection
    ) -> Int? {
        guard pageCount > 0, (0..<pageCount).contains(failedIndex) else { return nil }
        let primary: [Int]
        let fallback: [Int]
        switch direction {
        case .forward:
            primary = Array((failedIndex + 1)..<pageCount)
            fallback = failedIndex > 0 ? Array(stride(from: failedIndex - 1, through: 0, by: -1)) : []
        case .backward:
            primary = failedIndex > 0 ? Array(stride(from: failedIndex - 1, through: 0, by: -1)) : []
            fallback = Array((failedIndex + 1)..<pageCount)
        }
        return (primary + fallback).first { !failedIndices.contains($0) }
    }
}

/// Keeps UIKit picker delegate callbacks idempotent. Some providers dismiss while
/// also completing an outstanding callback; the SwiftUI presentation must only be
/// torn down once.
final class PickerCompletionGate {
    private(set) var isCompleted = false

    @discardableResult
    func perform(_ completion: () -> Void) -> Bool {
        guard !isCompleted else { return false }
        isCompleted = true
        completion()
        return true
    }
}

enum ViewerResponsiveLayout {
    static let pinnedFilmstripMinimumWidth: CGFloat = 900

    static func pinsFilmstrip(isPad: Bool, width: CGFloat, enabled: Bool) -> Bool {
        isPad && enabled && width >= pinnedFilmstripMinimumWidth
    }
}

enum CodecBackend: Equatable {
    case internalCodec
    case imageIO
}

enum CodecRouting: String, CaseIterable {
    case `default` = "DEFAULT"
    case internalFirst = "INTERNAL_FIRST"
    case osFirst = "OS_FIRST"
    case internalOnly = "INTERNAL_ONLY"
    case osOnly = "OS_ONLY"

    init(configValue: String) {
        self = CodecRouting(rawValue: configValue) ?? .default
    }

    var decodeOrder: [CodecBackend] {
        switch self {
        case .default, .internalFirst: [.internalCodec, .imageIO]
        case .osFirst: [.imageIO, .internalCodec]
        case .internalOnly: [.internalCodec]
        case .osOnly: [.imageIO]
        }
    }
}

enum ThemeMode: String, Codable, CaseIterable {
    case cinematicDark, light, system

    var colorScheme: ColorScheme? {
        switch self {
        case .cinematicDark: .dark
        case .light: .light
        case .system: nil
        }
    }
}

struct MobileConfigV1: Codable, Equatable {
    var schemaVersion = 1
    var fit: DisplayFit = .contain
    var showTopChrome = true
    var showFilmstrip = true
    var keepScreenOn = false
    var mangaEnabled = false
    var mangaRTL = true
    var coverAlone = true
    var prefetchSpreads = 1
    var mangaPageSpacing = MangaPageSpacing.defaultPoints
    var theme: ThemeMode = .cinematicDark
    var language = "system"
    var rememberLastLocation = true
    var cacheLimitBytes: UInt64? = nil
    var codecRouting = "DEFAULT"
    var touchZonesEnabled = true
    var swipeEnabled = false
    var pinchZoomEnabled = true
    var panEnabled = true
    var longPressQuickMenuEnabled = true

    var locale: Locale {
        language == "system" ? .autoupdatingCurrent : Locale(identifier: language)
    }

    private enum CodingKeys: String, CodingKey {
        case schemaVersion, fit, showTopChrome, showFilmstrip, keepScreenOn, mangaEnabled, mangaRTL,
             coverAlone, prefetchSpreads, mangaPageSpacing, theme, language, rememberLastLocation, cacheLimitBytes,
             codecRouting, touchZonesEnabled, swipeEnabled, pinchZoomEnabled, panEnabled,
             longPressQuickMenuEnabled
    }

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        schemaVersion = try c.decodeIfPresent(Int.self, forKey: .schemaVersion) ?? 1
        fit = try c.decodeIfPresent(DisplayFit.self, forKey: .fit) ?? .contain
        showTopChrome = try c.decodeIfPresent(Bool.self, forKey: .showTopChrome) ?? true
        showFilmstrip = try c.decodeIfPresent(Bool.self, forKey: .showFilmstrip) ?? true
        keepScreenOn = try c.decodeIfPresent(Bool.self, forKey: .keepScreenOn) ?? false
        mangaEnabled = try c.decodeIfPresent(Bool.self, forKey: .mangaEnabled) ?? false
        mangaRTL = try c.decodeIfPresent(Bool.self, forKey: .mangaRTL) ?? true
        coverAlone = try c.decodeIfPresent(Bool.self, forKey: .coverAlone) ?? true
        prefetchSpreads = try c.decodeIfPresent(Int.self, forKey: .prefetchSpreads) ?? 1
        mangaPageSpacing = MangaPageSpacing.clamp(
            try c.decodeIfPresent(Double.self, forKey: .mangaPageSpacing)
                ?? MangaPageSpacing.defaultPoints
        )
        theme = try c.decodeIfPresent(ThemeMode.self, forKey: .theme) ?? .cinematicDark
        language = try c.decodeIfPresent(String.self, forKey: .language) ?? "system"
        rememberLastLocation = try c.decodeIfPresent(Bool.self, forKey: .rememberLastLocation) ?? true
        cacheLimitBytes = try c.decodeIfPresent(UInt64.self, forKey: .cacheLimitBytes)
        codecRouting = try c.decodeIfPresent(String.self, forKey: .codecRouting) ?? "DEFAULT"
        touchZonesEnabled = try c.decodeIfPresent(Bool.self, forKey: .touchZonesEnabled) ?? true
        swipeEnabled = try c.decodeIfPresent(Bool.self, forKey: .swipeEnabled) ?? false
        pinchZoomEnabled = try c.decodeIfPresent(Bool.self, forKey: .pinchZoomEnabled) ?? true
        panEnabled = try c.decodeIfPresent(Bool.self, forKey: .panEnabled) ?? true
        longPressQuickMenuEnabled = try c.decodeIfPresent(Bool.self, forKey: .longPressQuickMenuEnabled) ?? true
    }
}

enum ViewerAction: Equatable {
    case previous
    case next
    case openFiler
    case settings
    case filmstrip
}

struct TouchZone: Equatable {
    let row: Int
    let column: Int
}

enum TouchZoneResolver {
    static func zone(at point: CGPoint, in size: CGSize) -> TouchZone? {
        guard size.width > 0, size.height > 0,
              point.x >= 0, point.y >= 0,
              point.x < size.width, point.y < size.height else { return nil }
        return TouchZone(
            row: min(2, Int(point.y / (size.height / 3))),
            column: min(2, Int(point.x / (size.width / 3)))
        )
    }

    /// Physical left/right placement deliberately does not mirror in RTL locales.
    static func defaultAction(row: Int, column: Int) -> ViewerAction? {
        guard (0..<3).contains(row), (0..<3).contains(column) else { return nil }
        if column == 0 { return .previous }
        if column == 2 { return .next }
        switch row {
        case 0: return .openFiler
        case 1: return .settings
        case 2: return .filmstrip
        default: return nil
        }
    }
}

struct BookmarkRecord: Codable, Equatable, Sendable {
    let sourceID: UUID
    let bookmark: Data
    let displayName: String
    let isFolder: Bool
    var opaqueEntryID: String?
    var logicalPageIndex: Int
    /// Optional manifest identity for provider-backed `.wmltxt` sources.
    /// Missing fields decode as nil for pre-manifest bookmarks.
    var listedManifestOpaqueEntryID: String? = nil
    var listedManifestFileName: String? = nil
}

struct PageItem: Identifiable, Hashable, Sendable {
    let id: String
    let url: URL
    let displayName: String
    let isArchive: Bool

    var isSupported: Bool {
        MobileFileTypePolicy.shared.isSupported(displayName)
    }
}
