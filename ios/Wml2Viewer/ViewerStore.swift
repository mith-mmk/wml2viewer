import CoreGraphics
import Foundation
import ImageIO
import SwiftUI
import UniformTypeIdentifiers
import CoreImage
import OSLog
import UIKit

@MainActor
final class ViewerStore: ObservableObject {
    struct ExportItem: Identifiable {
        let id = UUID()
        let url: URL
        let format: ImageIOCodecRouter.ExportFormat
    }

    private struct AnimationFrame {
        let image: CGImage
        let durationNanoseconds: UInt64
    }

    @Published private(set) var pages: [PageItem] = []
    @Published private(set) var currentIndex = 0
    @Published private(set) var image: CGImage?
    @Published private(set) var spreadImages: [CGImage] = []
    @Published private(set) var isLoading = false
    @Published private(set) var touchReady = false
    @Published var errorMessage: String?
    @Published var sourceNoticeMessage: String?
    @Published var showSettings = false
    @Published var showFilmstrip = false
    @Published var showQuickMenu = false
    @Published var exportItem: ExportItem?
    @Published private(set) var animationEnabled = true
    @Published var grayscaleEnabled = false
    @Published var pendingPicker: PickerPresentation?
    @Published private(set) var filesOpenPhase: FilesOpenFlowPhase = .idle
    @Published private(set) var sourceConnectionState: SourceConnectionState = .empty
    @Published private(set) var sourceOpeningProgress: SourceOpeningProgress?
    @Published private(set) var thumbnails: [String: CGImage] = [:]
    @Published var zoom: CGFloat = 1
    @Published var pan: CGSize = .zero
    @Published private(set) var runtimeFit: DisplayFit?
    @Published private(set) var config = MobileConfigV1()

    #if DEBUG
    @Published private(set) var uiTestPickerFixtureReady = false
    #endif

    private let bookmarks = BookmarkStore()
    private let configStore = ConfigStore()
    private var source: SecurityScopedDocumentSource?
    private var nativeSession: NativeSession?
    private var nativeArchive: NativeArchive?
    private var archiveURL: URL?
    private var archiveParentPages: [PageItem]?
    private var archiveEntryIndices: [Int] = []
    private var listedManifestItem: PageItem?
    private var loadTask: Task<Void, Never>?
    private var animationTask: Task<Void, Never>?
    private var animationFrames: [AnimationFrame] = []
    private var animationFrameGeneration = 0
    private var animationFrameViewport = 0
    private var animationFrameSpreadPosition = 0
    private var sourceOpenTask: Task<Void, Never>?
    private var sourceOpenCancellation: DocumentSourceCancellation?
    private var sourceOpenOperationID: UUID?
    private var sourceOpenPickerContext: (flowID: UUID, presentationID: UUID)?
    private var sourceGeneration = 0
    private var viewportGeneration = 0
    private var viewportSize: CGSize = .zero
    private var readingPlan: NativeReadingPlan?
    private var portraitByPageID: [String: Bool] = [:]
    private var pickerFlow = FilesOpenFlowMachine()
    private var folderAuthorizationContext: FolderAuthorizationContext?
    private var activeBookmark: ActiveBookmark?
    private var thumbnailRequests: Set<String> = []
    private var thumbnailTasks: [String: Task<Void, Never>] = [:]
    private(set) var currentScenePhase: ScenePhase = .active
    private var failedFolderPageIDs: Set<String> = []
    private var folderTraversalDirection: FolderTraversalDirection = .forward
    private var flowToFinishAfterDismissal: UUID?
    private var configSaveSequence: UInt64 = 0
    private var memoryWarningObserver: NSObjectProtocol?
    private let filesLog = Logger(
        subsystem: "io.github.mith-mmk.wml2viewer",
        category: "FilesOpenFlow"
    )

    init() {
        memoryWarningObserver = NotificationCenter.default.addObserver(
            forName: UIApplication.didReceiveMemoryWarningNotification,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor in
                self?.purgeNonCurrentDecodedState()
            }
        }
    }

    deinit {
        sourceOpenCancellation?.cancel()
        sourceOpenTask?.cancel()
        thumbnailTasks.values.forEach { $0.cancel() }
        if let memoryWarningObserver {
            NotificationCenter.default.removeObserver(memoryWarningObserver)
        }
    }

    #if DEBUG
    private var uiTestPickerInitialDirectoryURL: URL?
    private var uiTestForceSpread = false
    private let providerAcceptance = ProviderAcceptanceRecorder.fromProcessArguments()
    private var providerAcceptanceStarted = false
    #endif

    var isPickerPresented: Bool { filesOpenPhase.blocksViewerInput }

    private struct FolderAuthorizationContext {
        let selectedOpaqueEntryID: String
        let selectedFileName: String
    }

    private struct ActiveBookmark {
        let sourceID: UUID
        let data: Data
        let displayName: String
        let isFolder: Bool
    }

    func restoreLastSource() async {
        config = await configStore.load()
        await MaterializeCache.shared.setTotalLimit(config.cacheLimitBytes.map(Int.init))
        #if DEBUG
        // A physical-provider acceptance run must start from an empty source.
        // Restoring a previously opened single file can otherwise produce a
        // decode event before Files was opened and makes an untouched device
        // look as though the acceptance flow had partially progressed.
        if providerAcceptance != nil {
            return
        }
        if ProcessInfo.processInfo.environment["WML2VIEWER_UI_TEST_NO_RESTORE"] == "1" {
            return
        }
        #endif
        guard config.rememberLastLocation else { return }
        let records: [BookmarkRecord]
        do {
            records = try await bookmarks.load()
        } catch {
            markRestoreFailure()
            return
        }
        guard let record = records.first else { return }
        var stale = false
        do {
            let resolved = try URL(resolvingBookmarkData: record.bookmark, options: [.withoutUI, .withoutMounting], relativeTo: nil, bookmarkDataIsStale: &stale)
            let newSource = SecurityScopedDocumentSource(sourceID: record.sourceID, displayName: record.displayName, rootURL: resolved, isFolder: record.isFolder)
            var activeBookmarkData = record.bookmark
            if stale, let renewed = try? resolved.bookmarkData(options: [.suitableForBookmarkFile], includingResourceValuesForKeys: nil, relativeTo: nil) {
                activeBookmarkData = renewed
                try? await bookmarks.upsert(BookmarkRecord(sourceID: record.sourceID, bookmark: renewed, displayName: record.displayName, isFolder: record.isFolder, opaqueEntryID: record.opaqueEntryID, logicalPageIndex: record.logicalPageIndex, listedManifestOpaqueEntryID: record.listedManifestOpaqueEntryID, listedManifestFileName: record.listedManifestFileName))
            }
            activeBookmark = ActiveBookmark(
                sourceID: record.sourceID, data: activeBookmarkData,
                displayName: record.displayName, isFolder: record.isFolder
            )
            try await open(
                source: newSource,
                preferredOpaqueEntryID: record.opaqueEntryID,
                preferredFileName: nil,
                preferredIndex: record.logicalPageIndex,
                listedManifestOpaqueEntryID: record.listedManifestOpaqueEntryID,
                listedManifestFileName: record.listedManifestFileName
            )
        } catch {
            markRestoreFailure()
        }
    }

    /// A stale bookmark, revoked provider authorization, or an offline File
    /// Provider must not leave the viewer in a non-interactive loading state.
    /// Keep the current source (if any) and expose an OS-picker retry path.
    private func markRestoreFailure() {
        sourceConnectionState = .retryableError
        touchReady = true
        errorMessage = DocumentSourceError.permissionDenied.localizedDescription
    }

    #if DEBUG
    func startProviderAcceptanceIfNeeded() {
        guard providerAcceptance != nil,
              !providerAcceptanceStarted else { return }
        providerAcceptanceStarted = true
        requestFilePicker()
    }

    func applyUITestOverrides() {
        if let language = ProcessInfo.processInfo.environment["WML2VIEWER_UI_TEST_LANGUAGE"] {
            config.language = language
        }
        if ProcessInfo.processInfo.environment["WML2VIEWER_UI_TEST_HIDE_CHROME"] == "1" {
            config.showTopChrome = false
        }
        if let routing = ProcessInfo.processInfo.environment["WML2VIEWER_UI_TEST_CODEC_ROUTING"] {
            config.codecRouting = routing
        }
        if ProcessInfo.processInfo.environment["WML2VIEWER_UI_TEST_MANGA"] == "1" {
            config.mangaEnabled = true
            config.coverAlone = false
        }
        uiTestForceSpread = ProcessInfo.processInfo.environment[
            "WML2VIEWER_UI_TEST_FORCE_SPREAD"
        ] == "1"
        if let value = ProcessInfo.processInfo.environment["WML2VIEWER_UI_TEST_MANGA_PAGE_SPACING"],
           let spacing = Double(value) {
            config.mangaPageSpacing = MangaPageSpacing.clamp(spacing)
        }
    }

    var uiTestMangaSpreadReady: Bool { displaySpreadImages.count == 2 && touchReady }

    func installUITestFixtureIfRequested() async {
        let environment = ProcessInfo.processInfo.environment
        let folderFixture = environment["WML2VIEWER_UI_TEST_FIXTURE_FOLDER"] == "1"
        let errorFixture = environment["WML2VIEWER_UI_TEST_FIXTURE_UNSUPPORTED"] == "1"
        let archiveFormat = environment["WML2VIEWER_UI_TEST_FIXTURE_ARCHIVE"]
        let pickerFolderName = environment["WML2VIEWER_UI_TEST_PICKER_FOLDER_NAME"]
        let pickerIncludesUnsupported = environment[
            "WML2VIEWER_UI_TEST_PICKER_UNSUPPORTED_FILE"
        ] == "1"
        let folderIncludesEmptyArchive = environment[
            "WML2VIEWER_UI_TEST_FOLDER_EMPTY_ARCHIVE"
        ] == "1"
        guard folderFixture || errorFixture || archiveFormat != nil || pickerFolderName != nil || folderIncludesEmptyArchive else {
            return
        }
        do {
            if let pickerFolderName {
                try installUITestPickerFixture(
                    named: pickerFolderName,
                    includesUnsupportedFile: pickerIncludesUnsupported
                )
                return
            }
            let directory = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("ui-folder-fixture", isDirectory: true)
            try? FileManager.default.removeItem(at: directory)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            if errorFixture {
                try Data("not an image".utf8).write(
                    to: directory.appendingPathComponent("unsupported.png"),
                    options: .atomic
                )
            } else if let archiveFormat {
                let encoded: String
                switch archiveFormat.lowercased() {
                case "zip":
                    encoded = "UEsDBBQAAAAIAAAAIQAvZp0vSwAAAEYAAAALAAAAcGFnZS0wMS5wbmcBRgC5/4lQTkcNChoKAAAADUlIRFIAAAABAAAAAQgGAAAAHxXEiQAAAA1JREFUCNdj+M/A8B8ABQAB/4mZPR0AAAAASUVORK5CYIJQSwECFAMUAAAACAAAACEAL2adL0sAAABGAAAACwAAAAAAAAAAAAAApIEAAAAAcGFnZS0wMS5wbmdQSwUGAAAAAAEAAQA5AAAAdAAAAAAA"
                case "lzh", "lha":
                    // Hermetic Level 1 / LH5 archive containing two generated
                    // 640x400 MAG images. This intentionally exercises a real
                    // legacy-LHA wire stream instead of the former private LH0
                    // fixture that standard LHA tools rejected.
                    encoded = "JPgtbGg1LQMBAAAPBAIAZnZ9aiABC3BhZ2UtMDEubWFnz9tVAAACJ0tUoAh/3FJMi6zsrQIGZs2d4LLHiYgUMZG2eSjww+4BaDyuxwlbGM79JwhD0s8hrTtRnzveru/W+KIRrl96gAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD8AJJItbGg1LQMBAAAPBAIAZnZ9aiABC3BhZ2UtMDIubWFngMNVAAACJ0tQoAp/3ylOiWMqIEDM2/s/Q4FLHExAoYyNs5KOGHnALQeV2OFdlVUr85QhD0p8hrPtNHne9Hd+t8TQjbp96gAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD8AA=="
                default:
                    throw DocumentSourceError.unsupportedItem
                }
                guard let archiveData = Data(base64Encoded: encoded) else {
                    throw DocumentSourceError.unsupportedItem
                }
                let archiveURL = directory.appendingPathComponent("fixture.\(archiveFormat.lowercased())")
                try archiveData.write(to: archiveURL, options: .atomic)
                let fixture = SecurityScopedDocumentSource(
                    sourceID: UUID(), displayName: archiveURL.lastPathComponent,
                    rootURL: archiveURL, isFolder: false
                )
                source = fixture
                try await open(source: fixture, preferredIndex: 0)
                return
            }
            let colors = Self.uiTestFixtureColors
            for (index, color) in (folderFixture ? colors : []).enumerated() {
                let url = directory.appendingPathComponent(String(format: "page-%02d.png", index + 1))
                try Self.writeUITestPNG(color: color, to: url)
            }
            if folderIncludesEmptyArchive {
                // A valid empty ZIP is a supported container but has no
                // displayable entries. It must not strand a folder source on
                // a blank page; the viewer should advance to the next image.
                let emptyZip = Data(base64Encoded: "UEsFBgAAAAAAAAAAAAAAAAAAAAAAAA==")!
                try emptyZip.write(
                    to: directory.appendingPathComponent("00-empty.zip"),
                    options: .atomic
                )
            }
            let fixture = SecurityScopedDocumentSource(
                sourceID: UUID(), displayName: "UI fixture", rootURL: directory, isFolder: true
            )
            source = fixture
            try await open(source: fixture, preferredIndex: 0)
        } catch {
            errorMessage = DocumentSourceError.normalized(error).localizedDescription
        }
    }

    private static let uiTestFixtureColors: [[UInt8]] = [
        [0xE8, 0x4A, 0x5F, 0xFF],
        [0x4A, 0x90, 0xE2, 0xFF],
        [0x50, 0xC8, 0x78, 0xFF],
    ]

    private func installUITestPickerFixture(
        named folderName: String,
        includesUnsupportedFile: Bool
    ) throws {
        guard !folderName.isEmpty,
              folderName != ".",
              folderName != "..",
              !folderName.contains("/") else {
            throw CocoaError(.fileWriteInvalidFileName)
        }
        let documents = try FileManager.default.url(
            for: .documentDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let directory = documents.appendingPathComponent(folderName, isDirectory: true)
        if FileManager.default.fileExists(atPath: directory.path) {
            try FileManager.default.removeItem(at: directory)
        }
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
        for (index, color) in Self.uiTestFixtureColors.enumerated() {
            let url = directory.appendingPathComponent(String(format: "page-%02d.png", index + 1))
            try Self.writeUITestPNG(color: color, to: url)
        }
        if includesUnsupportedFile {
            try Data("unsupported fixture".utf8).write(
                to: directory.appendingPathComponent("unsupported.pdf"),
                options: .atomic
            )
        }
        uiTestPickerInitialDirectoryURL = documents
        uiTestPickerFixtureReady = true
    }

    private static func writeUITestPNG(color: [UInt8], to url: URL) throws {
        let data = Data(color)
        guard let provider = CGDataProvider(data: data as CFData),
              let image = CGImage(
                  width: 1, height: 1, bitsPerComponent: 8, bitsPerPixel: 32,
                  bytesPerRow: 4, space: CGColorSpaceCreateDeviceRGB(),
                  bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                  provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent
              ),
              let destination = CGImageDestinationCreateWithURL(
                  url as CFURL, UTType.png.identifier as CFString, 1, nil
              ) else {
            throw CocoaError(.fileWriteUnknown)
        }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else {
            throw CocoaError(.fileWriteUnknown)
        }
    }
    #endif

    func requestFilePicker() {
        #if DEBUG
        beginPicker(.openTarget, initialDirectoryURL: uiTestPickerInitialDirectoryURL)
        #else
        beginPicker(.openTarget)
        #endif
    }

    func requestFolderPicker() {
        folderAuthorizationContext = nil
        beginPicker(.containingFolder)
    }

    func requestFileManagement() {
        beginPicker(.manageFiles)
    }

    private func beginPicker(_ request: PickerRequest, initialDirectoryURL: URL? = nil) {
        errorMessage = nil
        sourceNoticeMessage = nil
        flowToFinishAfterDismissal = nil
        guard let presentation = pickerFlow.begin(
            request,
            initialDirectoryURL: initialDirectoryURL
        ) else { return }
        pendingPicker = presentation
        syncPickerPhase()
        #if DEBUG
        if request != .manageFiles { providerAcceptance?.pickerRequested() }
        #endif
    }

    func finishPicker(
        _ presentation: PickerPresentation,
        _ result: Result<URL, Error>?
    ) {
        guard pickerFlow.beginResultProcessing(presentation) else { return }
        pendingPicker = nil
        syncPickerPhase()
        guard let result else {
            #if DEBUG
            providerAcceptance?.pickerCancelled()
            #endif
            if presentation.request == .containingFolder,
               folderAuthorizationContext != nil {
                folderAuthorizationContext = nil
                errorMessage = nil
                sourceNoticeMessage = String(localized: "Folder selection was cancelled. The selected file remains open by itself.")
            } else {
                Task { await reconcileExternalChanges() }
            }
            finishFlowWhenDismissed(
                flowID: presentation.flowID,
                presentationID: presentation.id
            )
            return
        }
        let operationID = UUID()
        let cancellation = DocumentSourceCancellation()
        sourceOpenOperationID = operationID
        sourceOpenCancellation = cancellation
        sourceOpenPickerContext = (presentation.flowID, presentation.id)
        // Start the scope while the picker-delivered URL is still in the
        // delegate hand-off. Some third-party providers vend a transient URL;
        // retaining the scope across dismissal keeps that grant alive until a
        // bookmark and the first coordinated snapshot have both completed.
        let selectedURL = try? result.get()
        let retainedSelectionScope = selectedURL?.startAccessingSecurityScopedResource() == true
        sourceOpenTask = Task { @MainActor [weak self] in
            defer {
                if retainedSelectionScope, let selectedURL {
                    selectedURL.stopAccessingSecurityScopedResource()
                }
            }
            guard let self else { return }
            do {
                let url = try result.get()
                self.errorMessage = nil
                self.sourceNoticeMessage = nil
                self.sourceOpeningProgress = SourceOpeningProgress(
                    isFolder: presentation.request == .containingFolder,
                    processedItemCount: 0,
                    totalItemCount: nil
                )
                // A full-screen document picker covers the app's progress UI.
                // Wait for UIKit/SwiftUI dismissal before touching a provider;
                // otherwise a slow File Provider looks frozen and Cancel is
                // unreachable behind the picker scene.
                try await self.waitForPickerDismissal(presentation.id)
                try Task.checkCancellation()
                try cancellation.checkCancellation()
                guard self.sourceOpenOperationID == operationID else { return }
                let resourceIsDirectory = await Task.detached(priority: .userInitiated) {
                    Self.isDirectoryURL(url)
                }.value
                try Task.checkCancellation()
                try cancellation.checkCancellation()
                guard self.sourceOpenOperationID == operationID else { return }
                let isFolder = presentation.request.selectionIsFolder(
                    resourceIsDirectory: resourceIsDirectory
                )
                try SelectedDocumentPolicy.validate(url: url, isFolder: isFolder)
                self.sourceOpeningProgress = SourceOpeningProgress(
                    isFolder: isFolder,
                    processedItemCount: 0,
                    totalItemCount: nil
                )
                #if DEBUG
                if let delay = ProcessInfo.processInfo.environment[
                    "WML2VIEWER_UI_TEST_SOURCE_OPENING_DELAY_NANOSECONDS"
                ].flatMap(UInt64.init), delay > 0 {
                    try await Task.sleep(nanoseconds: delay)
                }
                #endif
                let authorization = presentation.request == .containingFolder
                    ? folderAuthorizationContext : nil
                if presentation.request == .containingFolder {
                    folderAuthorizationContext = nil
                }
                var snapshot = try await accept(
                    url: url,
                    isFolder: isFolder,
                    preferredOpaqueEntryID: authorization?.selectedOpaqueEntryID,
                    preferredFileName: authorization?.selectedFileName,
                    requiresPreferredItem: authorization != nil,
                    cancellation: cancellation,
                    progress: { [weak self] processed, total in
                        Task { @MainActor in
                            guard let self,
                                  self.sourceOpenOperationID == operationID,
                                  var progress = self.sourceOpeningProgress else { return }
                            progress.processedItemCount = processed
                            progress.totalItemCount = total
                            self.sourceOpeningProgress = progress
                        }
                    }
                )
                try Task.checkCancellation()
                try cancellation.checkCancellation()
                guard self.sourceOpenOperationID == operationID else { return }
                if let authorization,
                   presentation.request == .containingFolder,
                   MobileFileTypePolicy.shared.isListedFile(authorization.selectedFileName),
                   let folderSource = source,
                   let listedIndex = DocumentEntryMatcher.index(
                       selectedOpaqueEntryID: authorization.selectedOpaqueEntryID,
                       selectedFileName: authorization.selectedFileName,
                       in: pages
                   ),
                   pages.indices.contains(listedIndex) {
                    snapshot = try await openListedFile(
                        source: folderSource,
                        listedItem: pages[listedIndex]
                    )
                }
                filesLog.info(
                    "source committed: folder=\(isFolder, privacy: .public) enumerated=\(snapshot.enumeratedItemCount, privacy: .public) supported=\(snapshot.supportedItemCount, privacy: .public)"
                )
                #if DEBUG
                if isFolder { providerAcceptance?.folderCommitted(snapshot) }
                #endif
                if isFolder, snapshot.supportedItemCount == 1 {
                    sourceNoticeMessage = DocumentSourceError.noOtherSupportedItems.localizedDescription
                }
                let shouldRequestContainingFolder = presentation.request != .containingFolder &&
                    ContainingFolderAuthorizationPolicy.shouldRequest(
                       isFolder: isFolder,
                       isSupported: SelectedDocumentPolicy.isSupported(url: url),
                       isSelfContainedArchive: MobileFileTypePolicy.shared.isSelfContainedArchive(url.lastPathComponent)
                    )
                // A normal image must genuinely be visible before asking for
                // broader folder access. This also keeps corrupt images from
                // opening a second picker and then replacing their decode
                // error with a misleading folder-cancel notice. WMLTXT is the
                // exception: it cannot resolve any page until its containing
                // folder has been authorized.
                if !isFolder,
                   !MobileFileTypePolicy.shared.isListedFile(url.lastPathComponent) {
                    let displayed = try await waitForInitialDisplay(
                        operationID: operationID,
                        cancellation: cancellation
                    )
                    guard self.sourceOpenOperationID == operationID else { return }
                    if !displayed {
                        finishFlowWhenDismissed(
                            flowID: presentation.flowID,
                            presentationID: presentation.id
                        )
                        finishSourceOpening(operationID: operationID)
                        return
                    }
                }
                if shouldRequestContainingFolder {
                    queueContainingFolderAuthorization(
                        for: url,
                        flowID: presentation.flowID
                    )
                } else {
                    finishFlowWhenDismissed(
                        flowID: presentation.flowID,
                        presentationID: presentation.id
                    )
                }
                finishSourceOpening(operationID: operationID)
            } catch is CancellationError {
                guard self.sourceOpenOperationID == operationID else { return }
                finishCancelledSourceOpening(
                    operationID: operationID,
                    flowID: presentation.flowID,
                    presentationID: presentation.id
                )
            } catch {
                guard self.sourceOpenOperationID == operationID else { return }
                if presentation.request == .containingFolder {
                    folderAuthorizationContext = nil
                }
                sourceConnectionState = .retryableError
                errorMessage = DocumentSourceError.normalized(error).localizedDescription
                // A failed folder snapshot or an unsupported picker result
                // must never leave the gesture surface in its pre-open
                // loading state. Keep the previous source (if any) and make
                // Files/Settings available for retry.
                isLoading = false
                touchReady = true
                #if DEBUG
                providerAcceptance?.recoverableError(inputReady: interactionReady)
                #endif
                filesLog.error("source open failed in phase \(String(describing: presentation.request), privacy: .public)")
                finishFlowWhenDismissed(
                    flowID: presentation.flowID,
                    presentationID: presentation.id
                )
                finishSourceOpening(operationID: operationID)
            }
        }
    }

    func cancelSourceOpening() {
        guard sourceOpenOperationID != nil else { return }
        let pickerContext = sourceOpenPickerContext
        sourceOpenOperationID = nil
        sourceOpenCancellation?.cancel()
        sourceOpenTask?.cancel()
        sourceOpenTask = nil
        sourceOpenCancellation = nil
        sourceOpenPickerContext = nil
        sourceOpeningProgress = nil
        folderAuthorizationContext = nil
        errorMessage = nil
        sourceNoticeMessage = pages.isEmpty
            ? String(localized: "Opening was cancelled.")
            : String(localized: "Opening was cancelled. The current document remains open.")
        touchReady = true
        if let pickerContext {
            finishFlowWhenDismissed(
                flowID: pickerContext.flowID,
                presentationID: pickerContext.presentationID
            )
        }
        filesLog.info("source opening cancelled by user")
    }

    private func finishCancelledSourceOpening(
        operationID: UUID,
        flowID: UUID,
        presentationID: UUID
    ) {
        sourceNoticeMessage = pages.isEmpty
            ? String(localized: "Opening was cancelled.")
            : String(localized: "Opening was cancelled. The current document remains open.")
        touchReady = true
        finishFlowWhenDismissed(flowID: flowID, presentationID: presentationID)
        finishSourceOpening(operationID: operationID)
    }

    private func finishSourceOpening(operationID: UUID) {
        guard sourceOpenOperationID == operationID else { return }
        sourceOpenOperationID = nil
        sourceOpenTask = nil
        sourceOpenCancellation = nil
        sourceOpenPickerContext = nil
        sourceOpeningProgress = nil
    }

    private func waitForInitialDisplay(
        operationID: UUID,
        cancellation: DocumentSourceCancellation
    ) async throws -> Bool {
        while sourceOpenOperationID == operationID {
            try Task.checkCancellation()
            try cancellation.checkCancellation()
            if !isLoading {
                // `loadCurrent` makes decode failures interactive and writes a
                // precise, filename-bearing error. Preserve it rather than
                // converting the failure into a folder authorization prompt.
                return touchReady && image != nil && errorMessage == nil
            }
            try await ContinuousClock().sleep(for: .milliseconds(25))
        }
        throw CancellationError()
    }

    func pickerDidDismiss(_ presentationID: UUID) {
        let disappearedWithoutCallback = pendingPicker?.id == presentationID
        if disappearedWithoutCallback {
            // File Provider extensions can terminate while the system picker
            // is visible. SwiftUI may dismantle the controller without a
            // delegate callback, leaving the binding and flow in a stuck
            // presenting state. Treat that as a cancelled picker, preserve
            // the current source, and return the 3x3 surface to the user.
            pendingPicker = nil
            folderAuthorizationContext = nil
            sourceOpeningProgress = nil
            touchReady = true
            sourceNoticeMessage = pages.isEmpty
                ? String(localized: "Files closed before a selection was made.")
                : String(localized: "Files closed unexpectedly. The current document remains open.")
        }
        let next = pickerFlow.didDismiss(presentationID)
        if let next {
            pendingPicker = next
        } else if let flowID = flowToFinishAfterDismissal,
                  pickerFlow.flowID == flowID {
            flowToFinishAfterDismissal = nil
            pickerFlow.finish(flowID: flowID)
        } else if disappearedWithoutCallback,
                  let flowID = pickerFlow.flowID {
            // No result was delivered and no follow-up is pending.
            pickerFlow.finish(flowID: flowID)
        }
        syncPickerPhase()
    }

    private func waitForPickerDismissal(_ presentationID: UUID) async throws {
        let clock = ContinuousClock()
        let fallbackDeadline = clock.now.advanced(by: .seconds(1))
        while pickerFlow.activePresentationID == presentationID,
              !pickerFlow.activePresentationWasDismissed {
            try Task.checkCancellation()
            // SwiftUI normally reports the per-presentation `onDisappear`.
            // Real File Providers have been observed to finish their UIKit
            // dismissal without delivering the outer cover callback. Once the
            // binding is already nil and the animation had ample time to end,
            // infer dismissal so this task and the entire Files flow cannot
            // remain suspended forever.
            if pendingPicker == nil, clock.now >= fallbackDeadline {
                filesLog.warning("picker dismissal callback timed out; completing selected presentation")
                pickerDidDismiss(presentationID)
                break
            }
            try await clock.sleep(for: .milliseconds(25))
        }
        try Task.checkCancellation()
    }

    private func finishFlowWhenDismissed(flowID: UUID, presentationID: UUID) {
        guard pickerFlow.flowID == flowID,
              pickerFlow.activePresentationID == presentationID else { return }
        if pickerFlow.activePresentationWasDismissed {
            pickerFlow.finish(flowID: flowID)
            flowToFinishAfterDismissal = nil
        } else {
            flowToFinishAfterDismissal = flowID
        }
        syncPickerPhase()
    }

    private func syncPickerPhase() {
        filesOpenPhase = pickerFlow.phase
    }

    func openExternalURL(_ url: URL) {
        // External-open URLs have the same File Provider lifetime rules as
        // picker results. Retain their scope for classification, bookmark
        // creation and the initial coordinated snapshot.
        cancelSourceOpeningForReplacement()
        let operationID = UUID()
        let cancellation = DocumentSourceCancellation()
        sourceOpenOperationID = operationID
        sourceOpenCancellation = cancellation
        sourceOpenPickerContext = nil
        let retainedSelectionScope = url.startAccessingSecurityScopedResource()
        sourceOpenTask = Task { @MainActor [weak self] in
            defer {
                if retainedSelectionScope {
                    url.stopAccessingSecurityScopedResource()
                }
            }
            guard let self else { return }
            do {
                let isFolder = await Task.detached(priority: .userInitiated) {
                    Self.isDirectoryURL(url)
                }.value
                try Task.checkCancellation()
                try cancellation.checkCancellation()
                guard self.sourceOpenOperationID == operationID else { return }
                try SelectedDocumentPolicy.validate(url: url, isFolder: isFolder)
                self.errorMessage = nil
                self.sourceNoticeMessage = nil
                self.sourceOpeningProgress = SourceOpeningProgress(
                    isFolder: isFolder,
                    processedItemCount: 0,
                    totalItemCount: nil
                )
                let snapshot = try await self.accept(
                    url: url,
                    isFolder: isFolder,
                    cancellation: cancellation,
                    progress: { [weak self] processed, total in
                        Task { @MainActor in
                            guard let self,
                                  self.sourceOpenOperationID == operationID,
                                  var progress = self.sourceOpeningProgress else { return }
                            progress.processedItemCount = processed
                            progress.totalItemCount = total
                            self.sourceOpeningProgress = progress
                        }
                    }
                )
                try Task.checkCancellation()
                try cancellation.checkCancellation()
                guard self.sourceOpenOperationID == operationID else { return }
                #if DEBUG
                if isFolder { self.providerAcceptance?.folderCommitted(snapshot) }
                #endif
                let shouldRequestContainingFolder = ContainingFolderAuthorizationPolicy.shouldRequest(
                    isFolder: isFolder,
                    isSupported: SelectedDocumentPolicy.isSupported(url: url),
                    isSelfContainedArchive: MobileFileTypePolicy.shared.isSelfContainedArchive(url.lastPathComponent)
                )
                if !isFolder,
                   !MobileFileTypePolicy.shared.isListedFile(url.lastPathComponent) {
                    let displayed = try await self.waitForInitialDisplay(
                        operationID: operationID,
                        cancellation: cancellation
                    )
                    guard self.sourceOpenOperationID == operationID else { return }
                    if !displayed {
                        self.finishSourceOpening(operationID: operationID)
                        return
                    }
                }
                if shouldRequestContainingFolder {
                    self.queueContainingFolderAuthorization(for: url, flowID: nil)
                } else if isFolder, snapshot.supportedItemCount == 1 {
                    self.sourceNoticeMessage = DocumentSourceError.noOtherSupportedItems.localizedDescription
                }
                self.finishSourceOpening(operationID: operationID)
            } catch is CancellationError {
                guard self.sourceOpenOperationID == operationID else { return }
                self.sourceNoticeMessage = self.pages.isEmpty
                    ? String(localized: "Opening was cancelled.")
                    : String(localized: "Opening was cancelled. The current document remains open.")
                self.touchReady = true
                self.finishSourceOpening(operationID: operationID)
            } catch {
                guard self.sourceOpenOperationID == operationID else { return }
                self.sourceConnectionState = .retryableError
                self.errorMessage = DocumentSourceError.normalized(error).localizedDescription
                self.isLoading = false
                self.touchReady = true
                #if DEBUG
                self.providerAcceptance?.recoverableError(inputReady: self.interactionReady)
                #endif
                self.finishSourceOpening(operationID: operationID)
            }
        }
    }

    private func cancelSourceOpeningForReplacement() {
        guard sourceOpenOperationID != nil else { return }
        let pickerContext = sourceOpenPickerContext
        sourceOpenOperationID = nil
        sourceOpenCancellation?.cancel()
        sourceOpenTask?.cancel()
        sourceOpenTask = nil
        sourceOpenCancellation = nil
        sourceOpenPickerContext = nil
        sourceOpeningProgress = nil
        if let pickerContext {
            finishFlowWhenDismissed(
                flowID: pickerContext.flowID,
                presentationID: pickerContext.presentationID
            )
        }
    }

    func next() {
        guard !pages.isEmpty else { return }
        clearTransientSourceMessagesForNavigation()
        runtimeFit = nil
        let previousIndex = currentIndex
        folderTraversalDirection = .forward
        let proposedIndex = readingPlan?.nextAnchorIndex ?? min(currentIndex + 1, pages.count - 1)
        currentIndex = resolvedFolderNavigationIndex(
            proposedIndex,
            direction: .forward
        )
        #if DEBUG
        providerAcceptance?.navigated(from: previousIndex, to: currentIndex)
        #endif
        loadCurrent()
        persistCurrentLocation()
    }

    /// Executes an Android-compatible safe viewer action. File management
    /// remains OS-owned: the only file action exposed here is opening the
    /// mixed Files picker, while destructive operations stay in Document
    /// Browser's quick-menu entry.
    func perform(_ action: ViewerAction) {
        switch action {
        case .none:
            return
        case .previous:
            previous()
        case .next:
            next()
        case .first:
            select(index: 0)
        case .last:
            select(index: max(0, pages.count - 1))
        case .zoomIn:
            zoom = min(8, max(1, zoom * 1.25))
        case .zoomOut:
            zoom = max(1, zoom / 1.25)
            if zoom == 1 { pan = .zero }
        case .zoomReset:
            zoom = 1
            pan = .zero
        case .toggleFitMode:
            runtimeFit = (runtimeFit ?? config.fit) == .original ? .contain : .original
            zoom = 1
            pan = .zero
        case .toggleAnimation:
            toggleAnimation()
        case .toggleGrayscale:
            toggleGrayscale()
        case .toggleMangaMode:
            var updated = config
            updated.mangaEnabled.toggle()
            update(updated)
        case .openFiler:
            requestFilePicker()
        case .settings:
            showSettings = true
        case .filmstrip:
            openFilmstrip()
        case .openContextMenu:
            showQuickMenu = true
        case .export:
            prepareExport()
        case .reload:
            retryCurrentSource()
        }
    }

    func toggleAnimation() {
        animationEnabled.toggle()
        if !animationEnabled {
            animationTask?.cancel()
            animationTask = nil
        } else if !animationFrames.isEmpty {
            startAnimation(
                animationFrames,
                generation: animationFrameGeneration,
                viewport: animationFrameViewport,
                spreadPosition: animationFrameSpreadPosition
            )
        } else {
            loadCurrent()
        }
    }

    /// Stops work that is safe to defer while the scene is not visible. The
    /// current decoded spread remains published, while thumbnail and
    /// animation tasks are cancelled and can be recreated after activation.
    func handleScenePhase(_ phase: ScenePhase) {
        currentScenePhase = phase
        switch phase {
        case .active:
            guard animationEnabled, animationFrames.count > 1 else { return }
            startAnimation(
                animationFrames,
                generation: animationFrameGeneration,
                viewport: animationFrameViewport,
                spreadPosition: animationFrameSpreadPosition
            )
        case .inactive, .background:
            thumbnailTasks.values.forEach { $0.cancel() }
            thumbnailTasks.removeAll()
            thumbnailRequests.removeAll()
            animationTask?.cancel()
            animationTask = nil
        @unknown default:
            break
        }
    }

    func toggleGrayscale() {
        grayscaleEnabled.toggle()
    }

    var displayImage: CGImage? {
        guard grayscaleEnabled, let image else { return image }
        let ciImage = CIImage(cgImage: image)
        let filter = CIFilter(name: "CIColorControls")
        filter?.setValue(ciImage, forKey: kCIInputImageKey)
        filter?.setValue(0.0, forKey: kCIInputSaturationKey)
        guard let output = filter?.outputImage else { return image }
        return CIContext(options: nil).createCGImage(output, from: output.extent) ?? image
    }

    var displaySpreadImages: [CGImage] {
        guard grayscaleEnabled else { return spreadImages }
        return spreadImages.map(Self.grayscale)
    }

    var interactionReady: Bool { pages.isEmpty || touchReady }

    private static func grayscale(_ image: CGImage) -> CGImage {
        let ciImage = CIImage(cgImage: image)
        let filter = CIFilter(name: "CIColorControls")
        filter?.setValue(ciImage, forKey: kCIInputImageKey)
        filter?.setValue(0.0, forKey: kCIInputSaturationKey)
        guard let output = filter?.outputImage else { return image }
        return CIContext(options: nil).createCGImage(output, from: output.extent) ?? image
    }

    func prepareExport(format: ImageIOCodecRouter.ExportFormat = .png) {
        guard let image else { return }
        do {
            let directory = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("Exports", isDirectory: true)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let now = Date()
            for oldURL in try FileManager.default.contentsOfDirectory(
                at: directory,
                includingPropertiesForKeys: [.contentModificationDateKey],
                options: [.skipsHiddenFiles]
            ) {
                let modified = try? oldURL.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate
                if let modified, now.timeIntervalSince(modified) > 24 * 60 * 60 {
                    try? FileManager.default.removeItem(at: oldURL)
                }
            }
            let url = directory.appendingPathComponent(
                "page-\(UUID().uuidString).\(format.fileExtension)"
            )
            let data = try ImageIOCodecRouter.encode(image, format: format)
            try data.write(to: url, options: .atomic)
            exportItem = ExportItem(url: url, format: format)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func finishExport() {
        if let url = exportItem?.url { try? FileManager.default.removeItem(at: url) }
        exportItem = nil
    }

    private func purgeNonCurrentDecodedState() {
        thumbnails.removeAll(keepingCapacity: false)
        thumbnailRequests.removeAll(keepingCapacity: false)
        // Keep the visible image/spread interactive. In-flight work is
        // generation-guarded and will repopulate only after a fresh request.
        loadTask?.cancel()
        animationTask?.cancel()
        touchReady = !pages.isEmpty
    }

    func previous() {
        guard !pages.isEmpty else { return }
        clearTransientSourceMessagesForNavigation()
        runtimeFit = nil
        let previousIndex = currentIndex
        folderTraversalDirection = .backward
        let proposedIndex = readingPlan?.previousAnchorIndex ?? max(currentIndex - 1, 0)
        currentIndex = resolvedFolderNavigationIndex(
            proposedIndex,
            direction: .backward
        )
        #if DEBUG
        providerAcceptance?.navigated(from: previousIndex, to: currentIndex)
        #endif
        loadCurrent()
        persistCurrentLocation()
    }

    func openFilmstrip() {
        showFilmstrip = true
        #if DEBUG
        providerAcceptance?.filmstripOpened()
        #endif
    }

    var pagePositionAccessibilityValue: String {
        guard !pages.isEmpty else { return "0 / 0" }
        return "\(currentIndex + 1) / \(pages.count)"
    }

    func select(index: Int) {
        guard pages.indices.contains(index) else { return }
        clearTransientSourceMessagesForNavigation()
        runtimeFit = nil
        folderTraversalDirection = index < currentIndex ? .backward : .forward
        currentIndex = index
        showFilmstrip = false
        loadCurrent()
        persistCurrentLocation()
    }

    private func clearTransientSourceMessagesForNavigation() {
        errorMessage = nil
        sourceNoticeMessage = nil
    }

    func thumbnail(for item: PageItem) -> CGImage? {
        thumbnails[item.id]
    }

    func requestThumbnail(for item: PageItem) {
        guard currentScenePhase == .active,
              thumbnails[item.id] == nil,
              !thumbnailRequests.contains(item.id),
              let pageIndex = pages.firstIndex(where: { $0.id == item.id }) else { return }
        thumbnailRequests.insert(item.id)
        let generation = sourceGeneration
        let task = Task { [weak self] in
            guard let self else { return }
            defer {
                self.thumbnailRequests.remove(item.id)
                self.thumbnailTasks[item.id] = nil
            }
            do {
                let decoded: CGImage
                if let archive = self.nativeArchive,
                   let session = self.nativeSession,
                   self.archiveEntryIndices.indices.contains(pageIndex) {
                    let frames = try self.decodeArchiveFrames(
                        archive: archive,
                        session: session,
                        index: self.archiveEntryIndices[pageIndex],
                        entryName: item.displayName
                    )
                    guard let first = frames.first else { return }
                    decoded = first.image
                } else {
                    guard let source = self.source else { return }
                    let data = try await source.read(item)
                    if let image = Self.decodeThumbnail(data: data) {
                        decoded = image
                    } else {
                        let frames = try await self.decodeDocumentFrames(data: data, item: item)
                        guard let first = frames.first else { return }
                        decoded = first.image
                    }
                }
                guard !Task.isCancelled,
                      generation == self.sourceGeneration else { return }
                self.thumbnails[item.id] = Self.scaleThumbnail(decoded, maximumPixelSize: 160)
                #if DEBUG
                self.providerAcceptance?.thumbnailDecoded()
                #endif
            } catch {
                // A failed thumbnail must not remove the page or block navigation.
            }
        }
        thumbnailTasks[item.id] = task
    }

    func update(_ config: MobileConfigV1) {
        var config = config
        config.mangaPageSpacing = MangaPageSpacing.clamp(config.mangaPageSpacing)
        let readingChanged = config.mangaEnabled != self.config.mangaEnabled ||
            config.mangaRTL != self.config.mangaRTL || config.coverAlone != self.config.coverAlone
        self.config = config
        configSaveSequence &+= 1
        let saveSequence = configSaveSequence
        Task { try? await configStore.save(config, sequence: saveSequence) }
        if readingChanged { loadCurrent() }
    }

    func updateViewport(_ size: CGSize) {
        guard size.width > 0, size.height > 0, size != viewportSize else { return }
        viewportSize = size
        viewportGeneration &+= 1
        touchReady = false
        if !pages.isEmpty { loadCurrent() }
    }

    func reconcileExternalChanges() async {
        guard nativeArchive == nil, let source else { return }
        let oldIndex = currentIndex
        let oldID = pages.indices.contains(oldIndex) ? pages[oldIndex].id : nil
        do {
            let snapshot = try await source.snapshot()
            let refreshed = snapshot.entries
            guard !refreshed.isEmpty else {
                pages = []
                currentIndex = 0
                image = nil
                spreadImages = []
                touchReady = false
                sourceConnectionState = .retryableError
                errorMessage = source.isFolder
                    ? DocumentSourceError.noSupportedItems.localizedDescription
                    : DocumentSourceError.unsupportedItem.localizedDescription
                return
            }
            if source.isFolder,
               let previousManifest = listedManifestItem,
               let manifestIndex = DocumentEntryMatcher.index(
                   selectedOpaqueEntryID: DocumentEntryIdentity.opaqueIdentifier(for: previousManifest.url),
                   selectedFileName: previousManifest.displayName,
                   in: refreshed
               ),
               MobileFileTypePolicy.shared.isListedFile(refreshed[manifestIndex].displayName) {
                let manifestItem = refreshed[manifestIndex]
                let (_, entries) = try await listedEntries(source: source, listedItem: manifestItem)
                let refreshedCurrentID = pages.indices.contains(oldIndex) ? pages[oldIndex].id : nil
                let nextIndex = ExternalPageReconciler.index(
                    oldIndex: oldIndex,
                    oldID: refreshedCurrentID,
                    refreshedIDs: entries.map(\.id)
                ) ?? 0
                try commitOpen(source: source, listedPages: entries, preferredIndex: nextIndex)
                listedManifestItem = manifestItem
                sourceConnectionState = .archive(entries: entries.count)
                persistCurrentLocation()
                return
            }
            listedManifestItem = nil
            pages = refreshed
            sourceConnectionState = source.isFolder
                ? .folder(
                    enumerated: snapshot.enumeratedItemCount,
                    supported: snapshot.supportedItemCount
                )
                : .singleFile
            currentIndex = ExternalPageReconciler.index(
                oldIndex: oldIndex, oldID: oldID, refreshedIDs: refreshed.map(\.id)
            ) ?? 0
            sourceGeneration &+= 1
            loadCurrent()
            persistCurrentLocation()
        } catch {
            sourceConnectionState = .retryableError
            touchReady = true
            errorMessage = DocumentSourceError.normalized(error).localizedDescription
        }
    }

    /// Retries the current provider-backed source without discarding the
    /// selected folder or single-file state. If no source remains, fall back
    /// to the OS picker so the user can re-authorize or choose another item.
    func retryCurrentSource() {
        errorMessage = nil
        sourceNoticeMessage = nil
        guard source != nil else {
            requestFilePicker()
            return
        }
        if nativeArchive != nil {
            loadCurrent()
            return
        }
        Task { [weak self] in
            await self?.reconcileExternalChanges()
        }
    }

    private func accept(
        url: URL,
        isFolder: Bool,
        preferredOpaqueEntryID: String? = nil,
        preferredFileName: String? = nil,
        requiresPreferredItem: Bool = false,
        cancellation: DocumentSourceCancellation? = nil,
        progress: (@Sendable (_ processed: Int, _ total: Int) -> Void)? = nil
    ) async throws -> SourceSnapshot {
        try SelectedDocumentPolicy.validate(url: url, isFolder: isFolder)
        try cancellation?.checkCancellation()
        let bookmark = try await Task.detached(priority: .userInitiated) {
            let scoped = url.startAccessingSecurityScopedResource()
            defer { if scoped { url.stopAccessingSecurityScopedResource() } }
            return try url.bookmarkData(
                options: [.suitableForBookmarkFile],
                includingResourceValuesForKeys: nil,
                relativeTo: nil
            )
        }.value
        try cancellation?.checkCancellation()
        let newSource = SecurityScopedDocumentSource(sourceID: UUID(), displayName: url.lastPathComponent, rootURL: url, isFolder: isFolder)
        let snapshot: SourceSnapshot
        do {
            snapshot = try await newSource.snapshot(
                cancellation: cancellation,
                progress: progress
            )
        } catch {
            let cocoaError = error as NSError
            filesLog.error(
                "source snapshot failed: folder=\(isFolder, privacy: .public) domain=\(cocoaError.domain, privacy: .public) code=\(cocoaError.code, privacy: .public)"
            )
            throw error
        }
        filesLog.info(
            "source snapshot: folder=\(isFolder, privacy: .public) enumerated=\(snapshot.enumeratedItemCount, privacy: .public) supported=\(snapshot.supportedItemCount, privacy: .public)"
        )
        let listedPages = snapshot.entries
        guard !listedPages.isEmpty else {
            filesLog.error(
                "source snapshot empty: folder=\(isFolder, privacy: .public) enumerated=\(snapshot.enumeratedItemCount, privacy: .public)"
            )
            throw isFolder ? DocumentSourceError.noSupportedItems : DocumentSourceError.unsupportedItem
        }
        let preferredIndex: Int
        if let preferredFileName {
            if let matched = DocumentEntryMatcher.index(
                selectedOpaqueEntryID: preferredOpaqueEntryID,
                selectedFileName: preferredFileName,
                in: listedPages
            ) {
                preferredIndex = matched
            } else {
                guard !requiresPreferredItem else { throw DocumentSourceError.selectedFileNotFound }
                preferredIndex = 0
            }
        } else {
            preferredIndex = 0
        }
        try cancellation?.checkCancellation()
        try commitOpen(source: newSource, listedPages: listedPages, preferredIndex: preferredIndex)
        if let previous = activeBookmark, previous.sourceID != newSource.sourceID {
            try? await bookmarks.remove(sourceID: previous.sourceID)
        }
        activeBookmark = ActiveBookmark(
            sourceID: newSource.sourceID, data: bookmark,
            displayName: newSource.displayName, isFolder: isFolder
        )
        try await bookmarks.upsert(BookmarkRecord(
            sourceID: newSource.sourceID,
            bookmark: bookmark,
            displayName: newSource.displayName,
            isFolder: isFolder,
            opaqueEntryID: pages[currentIndex].id,
            logicalPageIndex: currentIndex,
            listedManifestOpaqueEntryID: listedManifestItem.map { DocumentEntryIdentity.opaqueIdentifier(for: $0.url) },
            listedManifestFileName: listedManifestItem?.displayName
        ))
        sourceConnectionState = isFolder
            ? .folder(
                enumerated: snapshot.enumeratedItemCount,
                supported: snapshot.supportedItemCount
            )
            : .singleFile
        return snapshot
    }

    private func queueContainingFolderAuthorization(
        for selectedURL: URL,
        flowID: UUID?
    ) {
        let scoped = selectedURL.startAccessingSecurityScopedResource()
        defer { if scoped { selectedURL.stopAccessingSecurityScopedResource() } }
        folderAuthorizationContext = FolderAuthorizationContext(
            selectedOpaqueEntryID: DocumentEntryIdentity.opaqueIdentifier(for: selectedURL),
            selectedFileName: selectedURL.lastPathComponent
        )
        let initialDirectoryURL = selectedURL.deletingLastPathComponent()
        if let flowID {
            guard pickerFlow.flowID == flowID else { return }
            if let presentation = pickerFlow.queueFollowUp(
                .containingFolder,
                initialDirectoryURL: initialDirectoryURL
            ) {
                pendingPicker = presentation
            }
            syncPickerPhase()
        } else {
            beginPicker(.containingFolder, initialDirectoryURL: initialDirectoryURL)
        }
    }

    private func persistCurrentLocation() {
        guard nativeArchive == nil,
              let activeBookmark,
              pages.indices.contains(currentIndex) else { return }
        let record = BookmarkRecord(
            sourceID: activeBookmark.sourceID,
            bookmark: activeBookmark.data,
            displayName: activeBookmark.displayName,
            isFolder: activeBookmark.isFolder,
            opaqueEntryID: pages[currentIndex].id,
            logicalPageIndex: currentIndex,
            listedManifestOpaqueEntryID: listedManifestItem.map { DocumentEntryIdentity.opaqueIdentifier(for: $0.url) },
            listedManifestFileName: listedManifestItem?.displayName
        )
        Task { try? await bookmarks.upsert(record) }
    }

    nonisolated private static func isDirectoryURL(_ url: URL) -> Bool {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        var isDirectory = ObjCBool(false)
        if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) {
            return isDirectory.boolValue
        }
        let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .contentTypeKey])
        return Self.isDirectoryResource(
            isDirectory: values?.isDirectory,
            contentType: values?.contentType,
            hasDirectoryPath: url.hasDirectoryPath
        )
    }

    nonisolated static func isDirectoryResource(
        isDirectory: Bool?,
        contentType: UTType?,
        hasDirectoryPath: Bool
    ) -> Bool {
        // Remote File Providers may return a placeholder URL for which
        // fileExists is false and isDirectory is nil. The declared UTType is
        // the remaining authoritative classification; do not treat such a
        // folder as a single unsupported file.
        isDirectory == true || contentType?.conforms(to: .folder) == true || hasDirectoryPath
    }

    private func open(
        source: SecurityScopedDocumentSource,
        preferredOpaqueEntryID: String? = nil,
        preferredFileName: String? = nil,
        preferredIndex: Int,
        listedManifestOpaqueEntryID: String? = nil,
        listedManifestFileName: String? = nil
    ) async throws {
        let snapshot = try await source.snapshot()
        let listedPages = snapshot.entries
        guard !listedPages.isEmpty else {
            throw source.isFolder
                ? DocumentSourceError.noSupportedItems
                : DocumentSourceError.unsupportedItem
        }
        let restoredIndex = preferredOpaqueEntryID.flatMap { opaqueID in
            DocumentEntryMatcher.index(
                selectedOpaqueEntryID: opaqueID,
                selectedFileName: preferredFileName ?? "",
                in: listedPages
            )
        } ?? preferredIndex
        try commitOpen(source: source, listedPages: listedPages, preferredIndex: restoredIndex)
        if source.isFolder,
           let manifestIndex = DocumentEntryMatcher.index(
               selectedOpaqueEntryID: listedManifestOpaqueEntryID,
               selectedFileName: listedManifestFileName ?? "",
               in: listedPages
           ),
           listedPages.indices.contains(manifestIndex),
           MobileFileTypePolicy.shared.isListedFile(listedPages[manifestIndex].displayName) {
            let manifestItem = listedPages[manifestIndex]
            let (_, entries) = try await listedEntries(source: source, listedItem: manifestItem)
            try commitOpen(
                source: source,
                listedPages: entries,
                preferredIndex: min(max(preferredIndex, 0), entries.count - 1)
            )
            listedManifestItem = manifestItem
            sourceConnectionState = .archive(entries: entries.count)
            persistCurrentLocation()
            return
        }
        sourceConnectionState = source.isFolder
            ? .folder(
                enumerated: snapshot.enumeratedItemCount,
                supported: snapshot.supportedItemCount
            )
            : .singleFile
    }

    /// Opens a `.wmltxt` manifest after the containing folder has been
    /// explicitly granted. Listed entries are resolved one-by-one under that
    /// folder and remain ordinary source pages, so each read still goes
    /// through the provider-aware `DocumentSource` contract.
    private func openListedFile(
        source: SecurityScopedDocumentSource,
        listedItem: PageItem
    ) async throws -> SourceSnapshot {
        let (paths, entries) = try await listedEntries(source: source, listedItem: listedItem)
        try commitOpen(source: source, listedPages: entries, preferredIndex: 0)
        listedManifestItem = listedItem
        sourceConnectionState = .archive(entries: entries.count)
        persistCurrentLocation()
        return SourceSnapshot(entries: entries, enumeratedItemCount: paths.count)
    }

    private func listedEntries(
        source: SecurityScopedDocumentSource,
        listedItem: PageItem
    ) async throws -> (paths: [String], entries: [PageItem]) {
        guard source.isFolder, MobileFileTypePolicy.shared.isListedFile(listedItem.displayName) else {
            throw DocumentSourceError.invalidListedFile
        }
        let manifest = try await source.read(listedItem)
        let paths = try WmltxtEntryResolver.paths(from: manifest)
        var entries: [PageItem] = []
        for path in paths {
            let url = try WmltxtEntryResolver.resolve(path, under: source.rootURL)
            let values = try? url.resourceValues(forKeys: [
                .isDirectoryKey, .isRegularFileKey, .nameKey, .contentTypeKey,
            ])
            let name = values?.name ?? path
            guard DirectoryEntryPolicy.includes(
                name: name,
                isDirectory: values?.isDirectory,
                isRegularFile: values?.isRegularFile,
                declaredMime: values?.contentType?.preferredMIMEType
            ) else { continue }
            entries.append(PageItem(
                id: DocumentEntryIdentity.opaqueIdentifier(for: url),
                url: url,
                displayName: path,
                isArchive: MobileFileTypePolicy.shared.isArchive(path)
            ))
        }
        guard !entries.isEmpty else { throw DocumentSourceError.noSupportedItems }
        return (paths, entries)
    }

    private func commitOpen(
        source: SecurityScopedDocumentSource,
        listedPages: [PageItem],
        preferredIndex: Int
    ) throws {
        let previousArchiveURL = archiveURL
        nativeArchive?.close()
        nativeSession?.close()
        nativeArchive = nil
        nativeSession = nil
        archiveURL = nil
        if let previousArchiveURL {
            Task { await MaterializeCache.shared.unpin(previousArchiveURL) }
        }
        archiveParentPages = nil
        listedManifestItem = nil
        sourceGeneration += 1
        portraitByPageID.removeAll()
        failedFolderPageIDs.removeAll()
        folderTraversalDirection = .forward
        thumbnails.removeAll()
        thumbnailRequests.removeAll()
        let generation = sourceGeneration
        self.source = source
        pages = listedPages
        currentIndex = min(max(preferredIndex, 0), pages.count - 1)
        guard generation == sourceGeneration else { return }
        loadCurrent()
    }

    private func loadCurrent() {
        loadTask?.cancel()
        animationTask?.cancel()
        animationFrames.removeAll(keepingCapacity: false)
        guard let source, pages.indices.contains(currentIndex) else {
            image = nil; spreadImages = []; touchReady = false; return
        }
        let item = pages[currentIndex]
        let index = currentIndex
        let generation = sourceGeneration
        let viewport = viewportGeneration
        let traversalDirection = folderTraversalDirection
        touchReady = false
        isLoading = true
        loadTask = Task { [weak self] in
            do {
                if let archive = self?.nativeArchive, let session = self?.nativeSession, let archiveURL = self?.archiveURL,
                   self?.archiveEntryIndices.indices.contains(index) == true {
                    let planned = self?.plannedIndices() ?? [index]
                    var decodedByIndex: [Int: [AnimationFrame]] = [:]
                    for plannedIndex in planned {
                        guard let self,
                              self.archiveEntryIndices.indices.contains(plannedIndex),
                              self.pages.indices.contains(plannedIndex) else { continue }
                        let archiveIndex = self.archiveEntryIndices[plannedIndex]
                        let entry = self.pages[plannedIndex]
                        do {
                            decodedByIndex[plannedIndex] = try self.decodeArchiveFrames(
                                archive: archive, session: session, index: archiveIndex,
                                entryName: entry.displayName
                            )
                        } catch {
                            // Prefetch/spread companions are opportunistic. A
                            // damaged neighbour must not turn the current valid
                            // page into a decode failure; it will report/skip
                            // when the user actually navigates to that entry.
                            if plannedIndex == index { throw error }
                        }
                    }
                    guard !Task.isCancelled else { return }
                    await MainActor.run {
                        guard let self, self.sourceGeneration == generation, self.viewportGeneration == viewport,
                              self.archiveURL == archiveURL else { return }
                        let visual = self.readingPlan?.visualIndices ?? [index]
                        let images = visual.compactMap { decodedByIndex[$0]?.first?.image }
                        guard let currentFrames = decodedByIndex[index], !images.isEmpty else { return }
                        for (pageIndex, frames) in decodedByIndex {
                            if let first = frames.first {
                                self.portraitByPageID[self.pages[pageIndex].id] = first.image.height >= first.image.width
                            }
                        }
                        let corrected = self.plannedIndices()
                        if corrected != planned {
                            DispatchQueue.main.async { self.loadCurrent() }
                            return
                        }
                        self.image = currentFrames[0].image
                        self.spreadImages = images
                        self.isLoading = false
                        self.touchReady = true
                        self.errorMessage = nil
                        self.sourceNoticeMessage = nil
                        #if DEBUG
                        self.providerAcceptance?.decodeReady(pageCount: self.pages.count)
                        #endif
                        let position = visual.firstIndex(of: index) ?? 0
                        self.startAnimation(currentFrames, generation: generation, viewport: viewport, spreadPosition: position)
                    }
                    return
                }
                if item.isArchive,
                   MobileFileTypePolicy.shared.isListedFile(item.displayName),
                   source.isFolder {
                    guard let self else { return }
                    let listedEntries = try await self.listedEntries(
                        source: source,
                        listedItem: item
                    )
                    guard !Task.isCancelled else { return }
                    await MainActor.run {
                        guard self.sourceGeneration == generation,
                              self.currentIndex == index else { return }
                        do {
                            try self.commitOpen(
                                source: source,
                                listedPages: listedEntries.entries,
                                preferredIndex: 0
                            )
                            self.listedManifestItem = item
                            self.sourceConnectionState = .archive(entries: listedEntries.entries.count)
                            self.persistCurrentLocation()
                        } catch {
                            if !self.skipFailedFolderPage(
                                item: item,
                                index: index,
                                generation: generation,
                                direction: traversalDirection
                            ) {
                                self.errorMessage = Self.userFacingDecodeError(
                                    error,
                                    displayName: item.displayName
                                ).localizedDescription
                                self.touchReady = true
                            }
                        }
                    }
                    return
                }
                if item.isArchive {
                    let data = try await source.read(item)
                    guard !Task.isCancelled else { return }
                    try await self?.openArchive(data: data, item: item, generation: generation)
                    return
                }
                    let planned = self?.plannedIndices() ?? [index]
                    var decodedByIndex: [Int: [AnimationFrame]] = [:]
                    for plannedIndex in planned {
                        guard let self, self.pages.indices.contains(plannedIndex) else { continue }
                        let plannedItem = self.pages[plannedIndex]
                        // Archives are standalone sources, never image frames
                        // in the surrounding folder's spread or prefetch.
                        // Trying to decode ZIP bytes as an image made an empty
                        // neighbouring archive blank an otherwise valid page.
                        guard !plannedItem.isArchive else { continue }
                        do {
                            let data = try await source.read(plannedItem)
                            decodedByIndex[plannedIndex] = try await self.decodeDocumentFrames(
                                data: data,
                                item: plannedItem
                            )
                        } catch {
                            if plannedIndex == index { throw error }
                        }
                    }
                guard !Task.isCancelled else { return }
                await MainActor.run {
                    guard let self, self.sourceGeneration == generation, self.viewportGeneration == viewport else { return }
                    let plan = self.readingPlan
                    let visual = plan?.visualIndices ?? [index]
                    let images = visual.compactMap { decodedByIndex[$0]?.first?.image }
                    guard !images.isEmpty, let currentFrames = decodedByIndex[index] else { return }
                    for (pageIndex, frames) in decodedByIndex {
                        if let first = frames.first {
                            self.portraitByPageID[self.pages[pageIndex].id] = first.image.height >= first.image.width
                        }
                    }
                    let corrected = self.plannedIndices()
                    if corrected != planned {
                        DispatchQueue.main.async { self.loadCurrent() }
                        return
                    }
                    self.image = currentFrames[0].image
                    self.spreadImages = images
                    self.isLoading = false
                    self.touchReady = true
                    self.errorMessage = nil
                    self.sourceNoticeMessage = nil
                    #if DEBUG
                    self.providerAcceptance?.decodeReady(pageCount: self.pages.count)
                    #endif
                    let position = visual.firstIndex(of: index) ?? 0
                    self.startAnimation(currentFrames, generation: generation, viewport: viewport, spreadPosition: position)
                }
            } catch {
                await MainActor.run { [weak self] in
                    guard let self else { return }
                    guard self.sourceGeneration == generation,
                          self.viewportGeneration == viewport,
                          self.currentIndex == index else { return }
                    // A failed decode must leave the surface interactive so the
                    // user can return to Files or Settings. Previously touchReady
                    // stayed false and every gesture was discarded, appearing as
                    // a frozen app after selecting an unsupported item.
                    self.isLoading = false
                    self.touchReady = true
                    self.image = nil
                    self.spreadImages = []
                    self.errorMessage = Self.userFacingDecodeError(
                        error,
                        displayName: item.displayName
                    ).localizedDescription
                    _ = self.skipFailedFolderPage(
                        item: item,
                        index: index,
                        generation: generation,
                        direction: traversalDirection
                    )
                    #if DEBUG
                    self.providerAcceptance?.recoverableError(inputReady: self.interactionReady)
                    #endif
                }
            }
        }
    }

    /// A folder is a continuous source: one damaged image or empty archive
    /// must not strand the viewer on a blank page. Skip each failed entry at
    /// most once, preserving the source and keeping the picker/settings
    /// gestures available. Single-file sources still show the precise error.
    @discardableResult
    private func skipFailedFolderPage(
        item: PageItem,
        index: Int,
        generation: Int,
        direction: FolderTraversalDirection
    ) -> Bool {
        guard source?.isFolder == true,
              sourceGeneration == generation,
              pages.count > 1,
              pages.indices.contains(index) else { return false }
        failedFolderPageIDs.insert(item.id)
        let failedIndices = Set(pages.indices.filter {
            failedFolderPageIDs.contains(pages[$0].id)
        })
        guard let next = FolderPageFailureNavigator.replacementIndex(
            failedIndex: index,
            pageCount: pages.count,
            failedIndices: failedIndices,
            direction: direction
        ) else { return false }
        sourceNoticeMessage = DocumentSourceError.unreadableDocument(item.displayName).localizedDescription
        errorMessage = nil
        currentIndex = next
        touchReady = false
        persistCurrentLocation()
        loadCurrent()
        return true
    }

    private func resolvedFolderNavigationIndex(
        _ proposedIndex: Int,
        direction: FolderTraversalDirection
    ) -> Int {
        guard source?.isFolder == true,
              pages.indices.contains(proposedIndex),
              failedFolderPageIDs.contains(pages[proposedIndex].id) else {
            return proposedIndex
        }
        let failedIndices = Set(pages.indices.filter {
            failedFolderPageIDs.contains(pages[$0].id)
        })
        return FolderPageFailureNavigator.replacementIndex(
            failedIndex: proposedIndex,
            pageCount: pages.count,
            failedIndices: failedIndices,
            direction: direction
        ) ?? currentIndex
    }

    private static func userFacingDecodeError(
        _ error: Error,
        displayName: String
    ) -> Error {
        if let documentError = error as? DocumentSourceError {
            switch documentError {
            case .unsupportedItem:
                return DocumentSourceError.unreadableDocument(displayName)
            default:
                return documentError
            }
        }
        if error is NativeBridgeError {
            return DocumentSourceError.unreadableDocument(displayName)
        }
        return DocumentSourceError.normalized(error)
    }

    private func plannedIndices() -> [Int] {
        guard config.mangaEnabled, !pages.isEmpty else {
            readingPlan = nil
            return [currentIndex]
        }
        let nativePages = pages.enumerated().map { index, page in
            NativeReadingPage(sourceID: 1, portrait: portraitByPageID[page.id] ?? true, cover: index == 0)
        }
        #if DEBUG
        let layout: NativeReadingLayout = uiTestForceSpread ? .spread : .auto
        #else
        let layout: NativeReadingLayout = .auto
        #endif
        let plan = NativeReadingPlanner.plan(
            pages: nativePages, currentIndex: currentIndex,
            landscape: viewportSize.width > viewportSize.height, layout: layout,
            direction: config.mangaRTL ? .rightToLeft : .leftToRight,
            coverAlone: config.coverAlone, maximumPrefetchSpreads: config.prefetchSpreads
        )
        readingPlan = plan
        return plan?.logicalIndices ?? [currentIndex]
    }

    private static func decodeFrames(data: Data, item: PageItem? = nil) throws -> [AnimationFrame] {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else {
            throw DocumentSourceError.unsupportedItem
        }
        let count = min(CGImageSourceGetCount(source), 256)
        if count <= 1, ImageIOCodecRouter.encodedAnimationHint(data) {
            throw DocumentSourceError.osAnimationUnsupported
        }
        var frames: [AnimationFrame] = []
        var retainedBytes: UInt64 = 0
        for index in 0..<count {
            guard let image = CGImageSourceCreateImageAtIndex(source, index, nil) else { continue }
            let bytes = UInt64(image.width) * UInt64(image.height) * 4
            let limit: UInt64 = 128 * 1_048_576
            guard bytes <= limit, retainedBytes <= limit - bytes else { break }
            retainedBytes += bytes
            let properties = CGImageSourceCopyPropertiesAtIndex(source, index, nil) as? [CFString: Any]
            let gif = properties?[kCGImagePropertyGIFDictionary] as? [CFString: Any]
            let delay = (gif?[kCGImagePropertyGIFUnclampedDelayTime] as? Double)
                ?? (gif?[kCGImagePropertyGIFDelayTime] as? Double)
                ?? 0.1
            frames.append(AnimationFrame(
                image: image,
                durationNanoseconds: UInt64(max(0.02, delay) * 1_000_000_000)
            ))
        }
        guard !frames.isEmpty else { throw DocumentSourceError.unsupportedItem }
        return frames
    }

    private static func decodeThumbnail(data: Data) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else { return nil }
        let options: [CFString: Any] = [
            kCGImageSourceCreateThumbnailFromImageAlways: true,
            kCGImageSourceThumbnailMaxPixelSize: 160,
            kCGImageSourceCreateThumbnailWithTransform: true,
            kCGImageSourceShouldCacheImmediately: false,
        ]
        return CGImageSourceCreateThumbnailAtIndex(source, 0, options as CFDictionary)
    }

    private static func scaleThumbnail(_ image: CGImage, maximumPixelSize: Int) -> CGImage {
        let longest = max(image.width, image.height)
        guard longest > maximumPixelSize else { return image }
        let scale = CGFloat(maximumPixelSize) / CGFloat(longest)
        let width = max(1, Int(CGFloat(image.width) * scale))
        let height = max(1, Int(CGFloat(image.height) * scale))
        guard let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { return image }
        context.interpolationQuality = .medium
        context.draw(image, in: CGRect(x: 0, y: 0, width: width, height: height))
        return context.makeImage() ?? image
    }

    private func decodeArchiveFrames(
        archive: NativeArchive, session: NativeSession, index: Int, entryName: String
    ) throws -> [AnimationFrame] {
        var lastError: Error = DocumentSourceError.unsupportedItem
        for backend in ImageIOCodecRouter.decodeOrder(routing: config.codecRouting) {
            do {
                let request = try session.nextRequest()
                switch backend {
                case .internalCodec:
                    let nativeImage = try archive.decode(
                        session: session, request: request, index: index,
                        mime: MobileFileTypePolicy.shared.mimeType(for: entryName)
                    )
                    defer { nativeImage.close() }
                    return try Self.decodeNativeFrames(nativeImage)
                case .imageIO:
                    let bytes = try archive.materialize(
                        session: session, request: request, index: index
                    )
                    defer { bytes.close() }
                    return try Self.decodeFrames(data: bytes.copy(), item: PageItem(
                        id: entryName, url: URL(fileURLWithPath: entryName),
                        displayName: entryName, isArchive: false
                    ))
                }
            } catch {
                lastError = error
            }
        }
        throw lastError
    }

    private func decodeDocumentFrames(data: Data, item: PageItem) async throws -> [AnimationFrame] {
        var lastError: Error = DocumentSourceError.unsupportedItem
        for backend in ImageIOCodecRouter.decodeOrder(routing: config.codecRouting) {
            do {
                switch backend {
                case .imageIO:
                    return try Self.decodeFrames(data: data, item: item)
                case .internalCodec:
                    let localURL = try await MaterializeCache.shared.materialize(
                        data,
                        suggestedExtension: item.url.pathExtension
                    )
                    await MaterializeCache.shared.pin(localURL)
                    defer { Task { await MaterializeCache.shared.unpin(localURL) } }
                    let session = try NativeSession()
                    defer { session.close() }
                    let request = try session.nextRequest()
                    let nativeImage = try NativeBridge.decode(path: localURL, session: session, request: request)
                    defer { nativeImage.close() }
                    return try Self.decodeNativeFrames(nativeImage)
                }
            } catch {
                lastError = error
            }
        }
        throw lastError
    }

    private static func decodeNativeFrames(_ image: NativeImage) throws -> [AnimationFrame] {
        var frames: [AnimationFrame] = []
        for index in 0..<max(1, image.frameCount) {
            let frame = image.frameCount > 1 ? try image.frame(at: index) : image
            let rgba = try frame.copyRGBA()
            let decoded = try decodeNativeRGBA(rgba, width: frame.width, height: frame.height, stride: frame.stride)
            let milliseconds = image.frameCount > 1 ? try image.frameDurationMilliseconds(at: index) : 100
            frames.append(AnimationFrame(image: decoded, durationNanoseconds: max(20, milliseconds) * 1_000_000))
            if frame !== image { frame.close() }
        }
        return frames
    }

    private func startAnimation(_ frames: [AnimationFrame], generation: Int, viewport: Int, spreadPosition: Int) {
        animationFrames = frames
        animationFrameGeneration = generation
        animationFrameViewport = viewport
        animationFrameSpreadPosition = spreadPosition
        animationTask?.cancel()
        guard animationEnabled, currentScenePhase == .active, frames.count > 1 else { return }
        animationTask = Task { [weak self] in
            var index = 0
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: frames[index].durationNanoseconds)
                guard !Task.isCancelled, let self, self.sourceGeneration == generation,
                      self.viewportGeneration == viewport else { return }
                index = (index + 1) % frames.count
                self.image = frames[index].image
                if self.spreadImages.indices.contains(spreadPosition) {
                    self.spreadImages[spreadPosition] = frames[index].image
                }
            }
        }
    }

    private func openArchive(data: Data, item: PageItem, generation: Int) async throws {
        let cache = try await MaterializeCache.shared.materialize(
            data, suggestedExtension: item.url.pathExtension.lowercased()
        )
        let session = try NativeSession()
        let request = try session.nextRequest()
        guard let format = MobileFileTypePolicy.shared.archiveFormat(for: item.displayName) else {
            throw DocumentSourceError.unsupportedItem
        }
        let archive = try NativeBridge.openArchive(
            path: cache, format: format, session: session, request: request
        )
        var entries: [PageItem] = []
        var entryIndices: [Int] = []
        for index in 0..<archive.entryCount {
            let name = try archive.entryName(at: index)
            guard MobileFileTypePolicy.shared.isImage(name) else { continue }
            entries.append(PageItem(id: "\(cache.path)#\(index)", url: URL(fileURLWithPath: name), displayName: name, isArchive: false))
            entryIndices.append(index)
        }
        guard !entries.isEmpty else { throw DocumentSourceError.unsupportedItem }
        await MaterializeCache.shared.pin(cache)
        await MainActor.run {
            guard self.sourceGeneration == generation else {
                Task { await MaterializeCache.shared.unpin(cache) }
                return
            }
            self.archiveParentPages = self.pages
            self.pages = entries
            self.currentIndex = 0
            self.archiveURL = cache
            self.nativeSession = session
            self.nativeArchive = archive
            self.archiveEntryIndices = entryIndices
            self.thumbnails.removeAll()
            self.thumbnailRequests.removeAll()
            self.sourceConnectionState = .archive(entries: entries.count)
        }
        loadCurrent()
    }

    func installTestPages(count: Int) {
        pages = (0..<count).map { index in
            PageItem(
                id: "test-\(index)", url: URL(fileURLWithPath: "/test-\(index).png"),
                displayName: "test-\(index).png", isArchive: false
            )
        }
        currentIndex = min(1, max(0, count - 1))
        portraitByPageID = Dictionary(uniqueKeysWithValues: pages.map { ($0.id, true) })
        viewportSize = CGSize(width: 800, height: 600)
        config.mangaEnabled = true
        config.mangaRTL = true
        config.coverAlone = true
    }

    var testReadingPlan: NativeReadingPlan? {
        _ = plannedIndices()
        return readingPlan
    }

    @discardableResult
    func installQueuedFolderPickerForTest(selectedURL: URL) -> UUID? {
        guard let primary = pickerFlow.begin(.openTarget) else { return nil }
        pendingPicker = primary
        _ = pickerFlow.beginResultProcessing(primary)
        pendingPicker = nil
        queueContainingFolderAuthorization(for: selectedURL, flowID: primary.flowID)
        syncPickerPhase()
        return primary.id
    }

    @discardableResult
    func installPendingFolderCancellationForTest(selectedURL: URL) -> PickerPresentation? {
        installTestPages(count: 2)
        touchReady = true
        folderAuthorizationContext = FolderAuthorizationContext(
            selectedOpaqueEntryID: DocumentEntryIdentity.opaqueIdentifier(for: selectedURL),
            selectedFileName: selectedURL.lastPathComponent
        )
        guard let presentation = pickerFlow.begin(
            .containingFolder,
            initialDirectoryURL: selectedURL.deletingLastPathComponent()
        ) else { return nil }
        pendingPicker = presentation
        syncPickerPhase()
        return presentation
    }

    @discardableResult
    func installPendingPickerForTest() -> PickerPresentation? {
        installTestPages(count: 2)
        touchReady = true
        guard let presentation = pickerFlow.begin(.openTarget) else { return nil }
        pendingPicker = presentation
        syncPickerPhase()
        return presentation
    }

    private static func decodeNativeRGBA(_ data: Data, width: Int, height: Int, stride: Int) throws -> CGImage {
        guard width > 0, height > 0, stride >= width * 4 else { throw NativeBridgeError.invalidBuffer }
        let provider = CGDataProvider(data: data as CFData)
        guard let provider else { throw NativeBridgeError.invalidBuffer }
        return CGImage(width: width, height: height, bitsPerComponent: 8, bitsPerPixel: 32,
                       bytesPerRow: stride, space: CGColorSpaceCreateDeviceRGB(),
                       bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                       provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent)!
    }
}
