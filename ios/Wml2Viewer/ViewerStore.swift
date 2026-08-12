import CoreGraphics
import Foundation
import ImageIO
import SwiftUI
import UniformTypeIdentifiers
import CoreImage

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
    @Published var showSettings = false
    @Published var showFilmstrip = false
    @Published var showQuickMenu = false
    @Published var exportItem: ExportItem?
    @Published private(set) var animationEnabled = true
    @Published var grayscaleEnabled = false
    @Published var pendingPicker: PickerRequest?
    @Published private(set) var isPickerPresented = false
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
            source = newSource
            if stale, let renewed = try? resolved.bookmarkData(options: [.suitableForBookmarkFile], includingResourceValuesForKeys: nil, relativeTo: nil) {
                try? await bookmarks.upsert(BookmarkRecord(sourceID: record.sourceID, bookmark: renewed, displayName: record.displayName, isFolder: record.isFolder, opaqueEntryID: record.opaqueEntryID, logicalPageIndex: record.logicalPageIndex))
            }
            try await open(source: newSource, preferredIndex: record.logicalPageIndex)
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
    }

    func installUITestFixtureIfRequested() async {
        guard ProcessInfo.processInfo.environment["WML2VIEWER_UI_TEST_FIXTURE_FOLDER"] == "1" else { return }
        do {
            let directory = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
                .appendingPathComponent("ui-folder-fixture", isDirectory: true)
            try? FileManager.default.removeItem(at: directory)
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
            let colors: [[UInt8]] = [[0xE8, 0x4A, 0x5F, 0xFF], [0x4A, 0x90, 0xE2, 0xFF], [0x50, 0xC8, 0x78, 0xFF]]
            for (index, color) in colors.enumerated() {
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
        pendingPicker = .file
        isPickerPresented = true
    }

    func requestFolderPicker() {
        pendingPicker = .folder
        isPickerPresented = true
    }

    func finishPicker(_ result: Result<URL, Error>?) {
        let completedRequest = pendingPicker
        pendingPicker = nil
        isPickerPresented = false
        guard let result else {
            Task { await reconcileExternalChanges() }
            return
        }
        Task { @MainActor in
            do {
                let url = try result.get()
                // An explicit folder picker is authoritative. File Provider URLs do
                // not have to encode directory-ness with a trailing slash.
                let resourceIsDirectory =
                    (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true
                let isFolder = completedRequest?.selectionIsFolder(
                    resourceIsDirectory: resourceIsDirectory
                ) ?? resourceIsDirectory
                try await accept(url: url, isFolder: isFolder)
            }
            catch { errorMessage = error.localizedDescription }
        }
    }

    func openExternalURL(_ url: URL) {
        Task { @MainActor in
            do {
                let values = try url.resourceValues(forKeys: [.isDirectoryKey])
                try await accept(url: url, isFolder: values.isDirectory == true)
            } catch {
                errorMessage = error.localizedDescription
            }
        }
    }

    func next() {
        guard !pages.isEmpty else { return }
        currentIndex = readingPlan?.nextAnchorIndex ?? min(currentIndex + 1, pages.count - 1)
        loadCurrent()
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
            let refreshed = try await source.list()
            guard !refreshed.isEmpty else {
                pages = []
                currentIndex = 0
                image = nil
                spreadImages = []
                touchReady = false
                errorMessage = DocumentSourceError.unsupportedItem.localizedDescription
                return
            }
            pages = refreshed
            currentIndex = ExternalPageReconciler.index(
                oldIndex: oldIndex, oldID: oldID, refreshedIDs: refreshed.map(\.id)
            ) ?? 0
            sourceGeneration &+= 1
            loadCurrent()
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func accept(url: URL, isFolder: Bool) async throws {
        let scoped = url.startAccessingSecurityScopedResource()
        defer { if scoped { url.stopAccessingSecurityScopedResource() } }
        let bookmark = try url.bookmarkData(options: [.suitableForBookmarkFile], includingResourceValuesForKeys: nil, relativeTo: nil)
        let newSource = SecurityScopedDocumentSource(sourceID: UUID(), displayName: url.lastPathComponent, rootURL: url, isFolder: isFolder)
        source = newSource
        try await bookmarks.upsert(BookmarkRecord(sourceID: newSource.sourceID, bookmark: bookmark, displayName: newSource.displayName, isFolder: isFolder, opaqueEntryID: nil, logicalPageIndex: 0))
        try await open(source: newSource, preferredIndex: 0)
    }

    private func open(source: SecurityScopedDocumentSource, preferredIndex: Int) async throws {
        nativeArchive?.close()
        nativeSession?.close()
        nativeArchive = nil
        nativeSession = nil
        archiveURL = nil
        archiveParentPages = nil
        sourceGeneration += 1
        portraitByPageID.removeAll()
        let generation = sourceGeneration
        pages = try await source.list()
        guard !pages.isEmpty else { throw DocumentSourceError.unsupportedItem }
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
                        guard let self, self.archiveEntryIndices.indices.contains(plannedIndex) else { continue }
                        let archiveIndex = self.archiveEntryIndices[plannedIndex]
                        let request = try session.nextRequest()
                        let nativeImage = try archive.decode(session: session, request: request, index: archiveIndex, mime: nil)
                        decodedByIndex[plannedIndex] = try Self.decodeNativeFrames(nativeImage)
                        nativeImage.close()
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
                    self?.isLoading = false
                    self?.errorMessage = error.localizedDescription
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
        let cache = FileManager.default.urls(for: .cachesDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("selected-\(UUID().uuidString).\(item.url.pathExtension)")
        try data.write(to: cache, options: .atomic)
        let session = try NativeSession()
        let request = try session.nextRequest()
        let archive = try NativeBridge.openArchive(path: cache, format: item.url.pathExtension.lowercased(), session: session, request: request)
        var entries: [PageItem] = []
        var entryIndices: [Int] = []
        for index in 0..<archive.entryCount {
            let name = try archive.entryName(at: index)
            let ext = URL(fileURLWithPath: name).pathExtension.lowercased()
            guard ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "avif", "heif", "heic", "wmltxt"].contains(ext) else { continue }
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
