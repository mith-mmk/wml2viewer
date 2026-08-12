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
    @Published private(set) var isLoading = false
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
        pendingPicker = nil
        isPickerPresented = false
        guard let result else { return }
        Task { @MainActor in
            do { try await accept(url: result.get(), isFolder: result.get().hasDirectoryPath) }
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
        currentIndex = min(currentIndex + 1, pages.count - 1)
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
        currentIndex = max(currentIndex - 1, 0)
        loadCurrent()
    }

    func select(index: Int) {
        guard pages.indices.contains(index) else { return }
        currentIndex = index
        showFilmstrip = false
        loadCurrent()
    }

    func update(_ config: MobileConfigV1) {
        self.config = config
        Task { try? await configStore.save(config) }
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
        guard let source, pages.indices.contains(currentIndex) else { image = nil; return }
        let item = pages[currentIndex]
        let index = currentIndex
        let generation = sourceGeneration
        isLoading = true
        loadTask = Task { [weak self] in
            do {
                if let archive = self?.nativeArchive, let session = self?.nativeSession, let archiveURL = self?.archiveURL,
                   let nativeIndex = self?.archiveEntryIndices.indices.contains(index) == true ? self?.archiveEntryIndices[index] : nil {
                    let request = try session.nextRequest()
                    let nativeImage = try archive.decode(session: session, request: request, index: nativeIndex, mime: nil)
                    let rgba = try nativeImage.copyRGBA()
                    let decoded = try Self.decodeNativeRGBA(rgba, width: nativeImage.width, height: nativeImage.height, stride: nativeImage.stride)
                    nativeImage.close()
                    guard !Task.isCancelled else { return }
                    await MainActor.run {
                        guard let self, self.sourceGeneration == generation, self.archiveURL == archiveURL else { return }
                        self.image = decoded
                        self.isLoading = false
                        self.errorMessage = nil
                    }
                    return
                }
                let data = try await source.read(item)
                guard !Task.isCancelled else { return }
                if item.isArchive {
                    try await self?.openArchive(data: data, item: item, generation: generation)
                    return
                }
                let frames = try Self.decodeFrames(data: data)
                await MainActor.run {
                    guard let self, self.sourceGeneration == generation else { return }
                    self.image = frames[0].image
                    self.isLoading = false
                    self.errorMessage = nil
                    self.startAnimation(frames, generation: generation)
                }
            } catch {
                await MainActor.run { [weak self] in
                    self?.isLoading = false
                    self?.errorMessage = error.localizedDescription
                }
            }
        }
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

    private func startAnimation(_ frames: [AnimationFrame], generation: Int) {
        guard animationEnabled, frames.count > 1 else { return }
        animationTask?.cancel()
        animationTask = Task { [weak self] in
            var index = 0
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: frames[index].durationNanoseconds)
                guard !Task.isCancelled, let self, self.sourceGeneration == generation else { return }
                index = (index + 1) % frames.count
                self.image = frames[index].image
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
