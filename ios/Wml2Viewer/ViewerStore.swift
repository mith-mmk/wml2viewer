import CoreGraphics
import Foundation
import ImageIO
import SwiftUI
import UniformTypeIdentifiers
import CoreImage
import OSLog

@MainActor
final class ViewerStore: ObservableObject {
    struct ExportItem: Identifiable {
        let id = UUID()
        let url: URL
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
    @Published private(set) var thumbnails: [String: CGImage] = [:]
    @Published var zoom: CGFloat = 1
    @Published var pan: CGSize = .zero
    @Published private(set) var config = MobileConfigV1()

    private let bookmarks = BookmarkStore()
    private let configStore = ConfigStore()
    private var source: SecurityScopedDocumentSource?
    private var nativeSession: NativeSession?
    private var nativeArchive: NativeArchive?
    private var archiveURL: URL?
    private var archiveParentPages: [PageItem]?
    private var archiveEntryIndices: [Int] = []
    private var loadTask: Task<Void, Never>?
    private var animationTask: Task<Void, Never>?
    private var sourceGeneration = 0
    private var viewportGeneration = 0
    private var viewportSize: CGSize = .zero
    private var readingPlan: NativeReadingPlan?
    private var portraitByPageID: [String: Bool] = [:]
    private var pickerFlow = FilesOpenFlowMachine()
    private var folderAuthorizationContext: FolderAuthorizationContext?
    private var activeBookmark: ActiveBookmark?
    private var thumbnailRequests: Set<String> = []
    private var flowToFinishAfterDismissal: UUID?
    private var lastPresentedPickerID: UUID?
    private let filesLog = Logger(
        subsystem: "io.github.mith-mmk.wml2viewer",
        category: "FilesOpenFlow"
    )

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
        #if DEBUG
        if ProcessInfo.processInfo.environment["WML2VIEWER_UI_TEST_NO_RESTORE"] == "1" {
            return
        }
        #endif
        guard config.rememberLastLocation else { return }
        guard let records = try? await bookmarks.load(), let record = records.first else { return }
        var stale = false
        do {
            let resolved = try URL(resolvingBookmarkData: record.bookmark, options: [.withoutUI, .withoutMounting], relativeTo: nil, bookmarkDataIsStale: &stale)
            let newSource = SecurityScopedDocumentSource(sourceID: record.sourceID, displayName: record.displayName, rootURL: resolved, isFolder: record.isFolder)
            var activeBookmarkData = record.bookmark
            if stale, let renewed = try? resolved.bookmarkData(options: [.suitableForBookmarkFile], includingResourceValuesForKeys: nil, relativeTo: nil) {
                activeBookmarkData = renewed
                try? await bookmarks.upsert(BookmarkRecord(sourceID: record.sourceID, bookmark: renewed, displayName: record.displayName, isFolder: record.isFolder, opaqueEntryID: record.opaqueEntryID, logicalPageIndex: record.logicalPageIndex))
            }
            activeBookmark = ActiveBookmark(
                sourceID: record.sourceID, data: activeBookmarkData,
                displayName: record.displayName, isFolder: record.isFolder
            )
            try await open(
                source: newSource,
                preferredOpaqueEntryID: record.opaqueEntryID,
                preferredFileName: nil,
                preferredIndex: record.logicalPageIndex
            )
        } catch { return }
    }

    #if DEBUG
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
    }

    func installUITestFixtureIfRequested() async {
        let environment = ProcessInfo.processInfo.environment
        let folderFixture = environment["WML2VIEWER_UI_TEST_FIXTURE_FOLDER"] == "1"
        let errorFixture = environment["WML2VIEWER_UI_TEST_FIXTURE_UNSUPPORTED"] == "1"
        let archiveFormat = environment["WML2VIEWER_UI_TEST_FIXTURE_ARCHIVE"]
        guard folderFixture || errorFixture || archiveFormat != nil else { return }
        do {
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
            let colors: [[UInt8]] = [[0xE8, 0x4A, 0x5F, 0xFF], [0x4A, 0x90, 0xE2, 0xFF], [0x50, 0xC8, 0x78, 0xFF]]
            for (index, color) in (folderFixture ? colors : []).enumerated() {
                let data = Data(color)
                guard let provider = CGDataProvider(data: data as CFData),
                      let image = CGImage(
                          width: 1, height: 1, bitsPerComponent: 8, bitsPerPixel: 32,
                          bytesPerRow: 4, space: CGColorSpaceCreateDeviceRGB(),
                          bitmapInfo: CGBitmapInfo(rawValue: CGImageAlphaInfo.premultipliedLast.rawValue),
                          provider: provider, decode: nil, shouldInterpolate: false, intent: .defaultIntent
                      ) else { continue }
                let url = directory.appendingPathComponent(String(format: "page-%02d.png", index + 1))
                guard let destination = CGImageDestinationCreateWithURL(url as CFURL, UTType.png.identifier as CFString, 1, nil) else { continue }
                CGImageDestinationAddImage(destination, image, nil)
                _ = CGImageDestinationFinalize(destination)
            }
            let fixture = SecurityScopedDocumentSource(
                sourceID: UUID(), displayName: "UI fixture", rootURL: directory, isFolder: true
            )
            source = fixture
            try await open(source: fixture, preferredIndex: 0)
        } catch {
            errorMessage = error.localizedDescription
        }
    }
    #endif

    func requestFilePicker() {
        beginPicker(.openTarget)
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
        lastPresentedPickerID = presentation.id
        syncPickerPhase()
    }

    func finishPicker(
        _ presentation: PickerPresentation,
        _ result: Result<URL, Error>?
    ) {
        guard pickerFlow.beginResultProcessing(presentation) else { return }
        pendingPicker = nil
        syncPickerPhase()
        guard let result else {
            if presentation.request == .containingFolder,
               folderAuthorizationContext != nil {
                folderAuthorizationContext = nil
                errorMessage = DocumentSourceError.folderRequired.localizedDescription
                sourceConnectionState = .retryableError
            } else {
                Task { await reconcileExternalChanges() }
            }
            finishFlowWhenDismissed(
                flowID: presentation.flowID,
                presentationID: presentation.id
            )
            return
        }
        Task { @MainActor in
            do {
                let url = try result.get()
                let resourceIsDirectory = Self.isDirectoryURL(url)
                let isFolder = presentation.request.selectionIsFolder(
                    resourceIsDirectory: resourceIsDirectory
                )
                let authorization = presentation.request == .containingFolder
                    ? folderAuthorizationContext : nil
                if presentation.request == .containingFolder {
                    folderAuthorizationContext = nil
                }
                let snapshot = try await accept(
                    url: url,
                    isFolder: isFolder,
                    preferredOpaqueEntryID: authorization?.selectedOpaqueEntryID,
                    preferredFileName: authorization?.selectedFileName,
                    requiresPreferredItem: authorization != nil
                )
                filesLog.info(
                    "source committed: folder=\(isFolder, privacy: .public) enumerated=\(snapshot.enumeratedItemCount, privacy: .public) supported=\(snapshot.supportedItemCount, privacy: .public)"
                )
                if isFolder, snapshot.supportedItemCount == 1 {
                    sourceNoticeMessage = DocumentSourceError.noOtherSupportedItems.localizedDescription
                }
                if presentation.request != .containingFolder,
                   ContainingFolderAuthorizationPolicy.shouldRequest(
                       isFolder: isFolder,
                       isSupported: MobileFileTypePolicy.shared.isSupported(url.lastPathComponent),
                       isArchive: MobileFileTypePolicy.shared.isArchive(url.lastPathComponent)
                   ) {
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
            } catch {
                if presentation.request == .containingFolder {
                    folderAuthorizationContext = nil
                }
                sourceConnectionState = .retryableError
                errorMessage = error.localizedDescription
                filesLog.error("source open failed in phase \(String(describing: presentation.request), privacy: .public)")
                finishFlowWhenDismissed(
                    flowID: presentation.flowID,
                    presentationID: presentation.id
                )
            }
        }
    }

    func pickerDidDismiss(_ presentationID: UUID) {
        let next = pickerFlow.didDismiss(presentationID)
        if let next {
            pendingPicker = next
            lastPresentedPickerID = next.id
        } else if let flowID = flowToFinishAfterDismissal,
                  pickerFlow.flowID == flowID {
            flowToFinishAfterDismissal = nil
            pickerFlow.finish(flowID: flowID)
        }
        syncPickerPhase()
    }

    func pickerCoverDidDismiss() {
        guard let lastPresentedPickerID else { return }
        pickerDidDismiss(lastPresentedPickerID)
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
        Task { @MainActor in
            do {
                let values = try url.resourceValues(forKeys: [.isDirectoryKey])
                let isFolder = values.isDirectory == true
                let snapshot = try await accept(url: url, isFolder: isFolder)
                if ContainingFolderAuthorizationPolicy.shouldRequest(
                    isFolder: isFolder,
                    isSupported: MobileFileTypePolicy.shared.isSupported(url.lastPathComponent),
                    isArchive: MobileFileTypePolicy.shared.isArchive(url.lastPathComponent)
                ) {
                    queueContainingFolderAuthorization(for: url, flowID: nil)
                } else if isFolder, snapshot.supportedItemCount == 1 {
                    sourceNoticeMessage = DocumentSourceError.noOtherSupportedItems.localizedDescription
                }
            } catch {
                sourceConnectionState = .retryableError
                errorMessage = error.localizedDescription
            }
        }
    }

    func next() {
        guard !pages.isEmpty else { return }
        currentIndex = readingPlan?.nextAnchorIndex ?? min(currentIndex + 1, pages.count - 1)
        loadCurrent()
        persistCurrentLocation()
    }

    func toggleAnimation() {
        animationEnabled.toggle()
        if !animationEnabled { animationTask?.cancel() } else { loadCurrent() }
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

    func prepareExport() {
        guard let image else { return }
        do {
            let directory = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("Exports", isDirectory: true)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let url = directory.appendingPathComponent("page-\(UUID().uuidString).png")
            guard let destination = CGImageDestinationCreateWithURL(
                url as CFURL, UTType.png.identifier as CFString, 1, nil
            ) else { throw DocumentSourceError.unsupportedItem }
            CGImageDestinationAddImage(destination, image, nil)
            guard CGImageDestinationFinalize(destination) else { throw DocumentSourceError.unsupportedItem }
            exportItem = ExportItem(url: url)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func finishExport() {
        if let url = exportItem?.url { try? FileManager.default.removeItem(at: url) }
        exportItem = nil
    }

    func previous() {
        guard !pages.isEmpty else { return }
        currentIndex = readingPlan?.previousAnchorIndex ?? max(currentIndex - 1, 0)
        loadCurrent()
        persistCurrentLocation()
    }

    var pagePositionAccessibilityValue: String {
        guard !pages.isEmpty else { return "0 / 0" }
        return "\(currentIndex + 1) / \(pages.count)"
    }

    func select(index: Int) {
        guard pages.indices.contains(index) else { return }
        currentIndex = index
        showFilmstrip = false
        loadCurrent()
        persistCurrentLocation()
    }

    func thumbnail(for item: PageItem) -> CGImage? {
        thumbnails[item.id]
    }

    func requestThumbnail(for item: PageItem) {
        guard thumbnails[item.id] == nil,
              !thumbnailRequests.contains(item.id),
              let pageIndex = pages.firstIndex(where: { $0.id == item.id }) else { return }
        thumbnailRequests.insert(item.id)
        let generation = sourceGeneration
        Task { [weak self] in
            guard let self else { return }
            defer { self.thumbnailRequests.remove(item.id) }
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
            } catch {
                // A failed thumbnail must not remove the page or block navigation.
            }
        }
    }

    func update(_ config: MobileConfigV1) {
        let readingChanged = config.mangaEnabled != self.config.mangaEnabled ||
            config.mangaRTL != self.config.mangaRTL || config.coverAlone != self.config.coverAlone
        self.config = config
        Task { try? await configStore.save(config) }
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
            errorMessage = error.localizedDescription
        }
    }

    private func accept(
        url: URL,
        isFolder: Bool,
        preferredOpaqueEntryID: String? = nil,
        preferredFileName: String? = nil,
        requiresPreferredItem: Bool = false
    ) async throws -> SourceSnapshot {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let bookmark = try url.bookmarkData(options: [.suitableForBookmarkFile], includingResourceValuesForKeys: nil, relativeTo: nil)
        let newSource = SecurityScopedDocumentSource(sourceID: UUID(), displayName: url.lastPathComponent, rootURL: url, isFolder: isFolder)
        let snapshot = try await newSource.snapshot()
        let listedPages = snapshot.entries
        guard !listedPages.isEmpty else {
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
            logicalPageIndex: currentIndex
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
                lastPresentedPickerID = presentation.id
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
            logicalPageIndex: currentIndex
        )
        Task { try? await bookmarks.upsert(record) }
    }

    private static func isDirectoryURL(_ url: URL) -> Bool {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        var isDirectory = ObjCBool(false)
        if FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) {
            return isDirectory.boolValue
        }
        return (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true || url.hasDirectoryPath
    }

    private func open(
        source: SecurityScopedDocumentSource,
        preferredOpaqueEntryID: String? = nil,
        preferredFileName: String? = nil,
        preferredIndex: Int
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
        sourceConnectionState = source.isFolder
            ? .folder(
                enumerated: snapshot.enumeratedItemCount,
                supported: snapshot.supportedItemCount
            )
            : .singleFile
    }

    private func commitOpen(
        source: SecurityScopedDocumentSource,
        listedPages: [PageItem],
        preferredIndex: Int
    ) throws {
        nativeArchive?.close()
        nativeSession?.close()
        nativeArchive = nil
        nativeSession = nil
        archiveURL = nil
        archiveParentPages = nil
        sourceGeneration += 1
        portraitByPageID.removeAll()
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
        guard let source, pages.indices.contains(currentIndex) else {
            image = nil; spreadImages = []; touchReady = false; return
        }
        let item = pages[currentIndex]
        let index = currentIndex
        let generation = sourceGeneration
        let viewport = viewportGeneration
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
                        decodedByIndex[plannedIndex] = try self.decodeArchiveFrames(
                            archive: archive, session: session, index: archiveIndex,
                            entryName: entry.displayName
                        )
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
                        let position = visual.firstIndex(of: index) ?? 0
                        self.startAnimation(currentFrames, generation: generation, viewport: viewport, spreadPosition: position)
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
                        let data = try await source.read(plannedItem)
                        decodedByIndex[plannedIndex] = try await self.decodeDocumentFrames(
                            data: data,
                            item: plannedItem
                        )
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
                    self.errorMessage = error.localizedDescription
                }
            }
        }
    }

    private func plannedIndices() -> [Int] {
        guard config.mangaEnabled, !pages.isEmpty else {
            readingPlan = nil
            return [currentIndex]
        }
        let nativePages = pages.enumerated().map { index, page in
            NativeReadingPage(sourceID: 1, portrait: portraitByPageID[page.id] ?? true, cover: index == 0)
        }
        let plan = NativeReadingPlanner.plan(
            pages: nativePages, currentIndex: currentIndex,
            landscape: viewportSize.width > viewportSize.height, layout: .auto,
            direction: config.mangaRTL ? .rightToLeft : .leftToRight,
            coverAlone: config.coverAlone, maximumPrefetchSpreads: config.prefetchSpreads
        )
        readingPlan = plan
        return plan?.logicalIndices ?? [currentIndex]
    }

    private static func decodeFrames(data: Data) throws -> [AnimationFrame] {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil) else {
            throw DocumentSourceError.unsupportedItem
        }
        let count = min(CGImageSourceGetCount(source), 256)
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
                    return try Self.decodeFrames(data: bytes.copy())
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
                    return try Self.decodeFrames(data: data)
                case .internalCodec:
                    let localURL = try await MaterializeCache.shared.materialize(
                        data,
                        suggestedExtension: item.url.pathExtension
                    )
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
        guard animationEnabled, frames.count > 1 else { return }
        animationTask?.cancel()
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
        await MainActor.run {
            guard self.sourceGeneration == generation else { return }
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
        lastPresentedPickerID = primary.id
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
        lastPresentedPickerID = presentation.id
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
