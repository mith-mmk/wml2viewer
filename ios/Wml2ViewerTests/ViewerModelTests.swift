import XCTest
@testable import Wml2Viewer

final class ViewerModelTests: XCTestCase {
    func testSpreadLayoutJoinsPagesAtBindingWhenSpacingIsZero() throws {
        let rects = SpreadLayout.pageRects(
            imageSizes: [CGSize(width: 600, height: 900), CGSize(width: 600, height: 900)],
            surfaceSize: CGSize(width: 1_400, height: 900),
            fit: .contain,
            spacing: 0
        )
        XCTAssertEqual(rects.count, 2)
        XCTAssertEqual(rects[0].maxX, rects[1].minX, accuracy: 0.001)
        XCTAssertEqual(rects[0].width, 600, accuracy: 0.001)
        XCTAssertEqual(rects[0].minX, 100, accuracy: 0.001)
        XCTAssertEqual(rects[1].maxX, 1_300, accuracy: 0.001)
    }

    func testSpreadLayoutAppliesOnlyConfiguredBindingSpacing() throws {
        let rects = SpreadLayout.pageRects(
            imageSizes: [CGSize(width: 600, height: 900), CGSize(width: 600, height: 900)],
            surfaceSize: CGSize(width: 1_400, height: 900),
            fit: .contain,
            spacing: 24
        )
        XCTAssertEqual(rects.count, 2)
        XCTAssertEqual(rects[1].minX - rects[0].maxX, 24, accuracy: 0.001)
        XCTAssertEqual(rects[0].minX, 88, accuracy: 0.001)
        XCTAssertEqual(rects[1].maxX, 1_312, accuracy: 0.001)
    }

    func testSpreadLayoutUsesOneScaleForDifferentPageSizes() throws {
        let rects = SpreadLayout.pageRects(
            imageSizes: [CGSize(width: 400, height: 800), CGSize(width: 600, height: 1_000)],
            surfaceSize: CGSize(width: 1_200, height: 1_000),
            fit: .contain,
            spacing: 0
        )
        XCTAssertEqual(rects[0].width, 400, accuracy: 0.001)
        XCTAssertEqual(rects[0].height, 800, accuracy: 0.001)
        XCTAssertEqual(rects[1].width, 600, accuracy: 0.001)
        XCTAssertEqual(rects[1].height, 1_000, accuracy: 0.001)
        XCTAssertEqual(rects[0].maxX, rects[1].minX, accuracy: 0.001)
        XCTAssertEqual(rects[0].midY, rects[1].midY, accuracy: 0.001)
    }

    func testMangaPageSpacingDefaultsAndClampsDuringConfigMigration() throws {
        let legacy = try JSONDecoder().decode(
            MobileConfigV1.self,
            from: Data(#"{"schemaVersion":1}"#.utf8)
        )
        XCTAssertEqual(legacy.mangaPageSpacing, 0)

        let oversized = try JSONDecoder().decode(
            MobileConfigV1.self,
            from: Data(#"{"schemaVersion":1,"mangaPageSpacing":999}"#.utf8)
        )
        XCTAssertEqual(oversized.mangaPageSpacing, MangaPageSpacing.maximumPoints)
        XCTAssertEqual(MangaPageSpacing.clamp(-20), MangaPageSpacing.minimumPoints)
        XCTAssertEqual(MangaPageSpacing.clamp(.infinity), MangaPageSpacing.defaultPoints)
    }

    func testConfigStoreRejectsLateOlderSpacingSave() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(".test-config-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = ConfigStore(
            fileURL: directory.appendingPathComponent("mobile-config-v1.json")
        )
        var latest = MobileConfigV1()
        latest.mangaPageSpacing = 12
        var stale = MobileConfigV1()
        stale.mangaPageSpacing = 4

        try await store.save(latest, sequence: 2)
        try await store.save(stale, sequence: 1)

        let restored = await store.load()
        XCTAssertEqual(restored.mangaPageSpacing, 12)
    }

    #if DEBUG
    func testProviderAcceptanceRequiresRealFolderNavigationAndFilmstripEvidence() {
        var report = ProviderAcceptanceReport(token: "acceptance-token", provider: .iCloud)
        XCTAssertEqual(report.status, "in-progress")

        report.recordPickerRequested()
        report.recordFolderSnapshot(enumerated: 5, supported: 3)
        report.recordDecodeReady(pageCount: 3)
        report.recordNavigation(from: 0, to: 1)
        report.recordFilmstripOpened()
        report.recordThumbnailDecoded()
        XCTAssertEqual(report.status, "in-progress", "forward-only navigation is insufficient")

        report.recordNavigation(from: 1, to: 0)
        XCTAssertEqual(report.status, "passed")
        XCTAssertEqual(report.provider, .iCloud)
        XCTAssertEqual(report.folderSupportedItemCount, 3)
        XCTAssertGreaterThan(report.sequence, 0)

        let encoded = try! JSONEncoder().encode(report)
        XCTAssertEqual(try! JSONDecoder().decode(ProviderAcceptanceReport.self, from: encoded), report)
        let keys = Set((try! JSONSerialization.jsonObject(with: encoded) as! [String: Any]).keys)
        XCTAssertTrue(keys.isDisjoint(with: ["url", "path", "fileName", "bookmark", "credential"]))
    }

    func testProviderAcceptanceTracksInteractiveErrorRecoverySeparately() {
        var report = ProviderAcceptanceReport(token: "error-token", provider: .smb)
        report.recordRecoverableError(inputReady: true)
        XCTAssertTrue(report.recoverableErrorObserved)
        XCTAssertTrue(report.inputReadyAfterError)
        XCTAssertFalse(report.recoveredAfterError)

        report.recordDecodeReady(pageCount: 2)
        XCTAssertTrue(report.recoveredAfterError)
        XCTAssertEqual(report.status, "in-progress")
    }
    #endif

    func testConfigRoundTrip() throws {
        struct Config: Codable, Equatable { var schemaVersion = 1; var rememberLastLocation = true }
        let config = Config()
        let data = try JSONEncoder().encode(config)
        XCTAssertEqual(try JSONDecoder().decode(Config.self, from: data), config)
    }

    func testNativeSessionOwnershipAndCancellation() throws {
        let session = try NativeSession()
        XCTAssertNotEqual(session.handle, 0)

        let request = try session.nextRequest()
        XCTAssertNotEqual(request, 0)
        session.cancel(request)

        session.close()
        XCTAssertEqual(session.handle, 0)
        session.close()
        XCTAssertEqual(session.handle, 0)
    }

    func testDefaultThreeByThreeTouchMapMatchesMobileContract() {
        XCTAssertEqual(TouchZoneResolver.defaultAction(row: 0, column: 0), .previous)
        XCTAssertEqual(TouchZoneResolver.defaultAction(row: 0, column: 1), .openFiler)
        XCTAssertEqual(TouchZoneResolver.defaultAction(row: 0, column: 2), .next)
        XCTAssertEqual(TouchZoneResolver.defaultAction(row: 1, column: 0), .previous)
        XCTAssertEqual(TouchZoneResolver.defaultAction(row: 1, column: 1), .settings)
        XCTAssertEqual(TouchZoneResolver.defaultAction(row: 1, column: 2), .next)
        XCTAssertEqual(TouchZoneResolver.defaultAction(row: 2, column: 0), .previous)
        XCTAssertEqual(TouchZoneResolver.defaultAction(row: 2, column: 1), .filmstrip)
        XCTAssertEqual(TouchZoneResolver.defaultAction(row: 2, column: 2), .next)
        XCTAssertNil(TouchZoneResolver.defaultAction(row: 3, column: 1))
    }

    func testWholeSurfaceLowerCenterCoordinateResolvesToFilmstripCell() {
        let size = CGSize(width: 390, height: 780)
        XCTAssertEqual(
            TouchZoneResolver.zone(at: CGPoint(x: 195, y: 650), in: size),
            TouchZone(row: 2, column: 1)
        )
        XCTAssertNil(TouchZoneResolver.zone(at: CGPoint(x: 195, y: 780), in: size))
    }

    func testConfiguredLanguageProducesExplicitLocale() {
        var config = MobileConfigV1()
        config.language = "ja"
        XCTAssertEqual(config.locale.language.languageCode?.identifier, "ja")
        config.language = "en"
        XCTAssertEqual(config.locale.language.languageCode?.identifier, "en")
    }

    func testGestureDefaultsMatchMobileContract() {
        let config = MobileConfigV1()
        XCTAssertFalse(config.swipeEnabled)
        XCTAssertTrue(config.pinchZoomEnabled)
        XCTAssertTrue(config.panEnabled)
        XCTAssertTrue(config.longPressQuickMenuEnabled)
    }

    func testPickerCompletionGateDeliversOnlyFirstProviderCallback() {
        let gate = PickerCompletionGate()
        var callbackCount = 0
        XCTAssertTrue(gate.perform { callbackCount += 1 })
        XCTAssertFalse(gate.perform { callbackCount += 1 })
        XCTAssertTrue(gate.isCompleted)
        XCTAssertEqual(callbackCount, 1)
    }

    func testFolderPickerContractDoesNotDependOnProviderURLTrailingSlash() {
        XCTAssertTrue(PickerRequest.containingFolder.selectionIsFolder(resourceIsDirectory: false))
        XCTAssertTrue(PickerRequest.openTarget.selectionIsFolder(resourceIsDirectory: true))
        XCTAssertFalse(PickerRequest.openTarget.selectionIsFolder(resourceIsDirectory: false))
        XCTAssertTrue(PickerRequest.openTarget.acceptsFolders)
        XCTAssertFalse(PickerRequest.manageFiles.acceptsFolders)
    }

    func testFilesOpenFlowSerializesFollowUpAndRejectsStaleCallbacks() {
        var flow = FilesOpenFlowMachine()
        let primary = try! XCTUnwrap(flow.begin(.openTarget))
        XCTAssertEqual(flow.phase, .presenting(.openTarget))
        XCTAssertTrue(flow.beginResultProcessing(primary))
        XCTAssertEqual(flow.phase, .processing(.openTarget))
        XCTAssertNil(flow.queueFollowUp(
            .containingFolder,
            initialDirectoryURL: URL(fileURLWithPath: "/provider/folder")
        ))

        let folder = try! XCTUnwrap(flow.didDismiss(primary.id))
        XCTAssertEqual(folder.request, .containingFolder)
        XCTAssertEqual(folder.flowID, primary.flowID)
        XCTAssertNotEqual(folder.id, primary.id)
        XCTAssertFalse(flow.beginResultProcessing(primary))
        XCTAssertTrue(flow.beginResultProcessing(folder))
        flow.finish(flowID: folder.flowID)
        XCTAssertEqual(flow.phase, .idle)
    }

    func testProviderPlaceholderDoesNotRequireRegularFileMetadata() {
        XCTAssertTrue(DirectoryEntryPolicy.includes(
            name: "cloud-page.png", isDirectory: false,
            isRegularFile: nil, declaredMime: "image/png"
        ))
        XCTAssertTrue(DirectoryEntryPolicy.includes(
            name: "smb-page.mag", isDirectory: nil,
            isRegularFile: nil, declaredMime: nil
        ))
        XCTAssertFalse(DirectoryEntryPolicy.includes(
            name: "looks-like-image.png", isDirectory: true,
            isRegularFile: nil, declaredMime: "image/png"
        ))
        XCTAssertFalse(DirectoryEntryPolicy.includes(
            name: "notes.txt", isDirectory: false,
            isRegularFile: nil, declaredMime: "text/plain"
        ))
    }

    func testContainingFolderAuthorizationOnlyFollowsOrdinaryImageSelection() {
        XCTAssertTrue(ContainingFolderAuthorizationPolicy.shouldRequest(
            isFolder: false, isSupported: true, isSelfContainedArchive: false
        ))
        XCTAssertFalse(ContainingFolderAuthorizationPolicy.shouldRequest(
            isFolder: true, isSupported: true, isSelfContainedArchive: false
        ))
        XCTAssertFalse(ContainingFolderAuthorizationPolicy.shouldRequest(
            isFolder: false, isSupported: true, isSelfContainedArchive: true
        ))
        XCTAssertFalse(ContainingFolderAuthorizationPolicy.shouldRequest(
            isFolder: false, isSupported: false, isSelfContainedArchive: false
        ))
        XCTAssertTrue(MobileFileTypePolicy.shared.isListedFile("book.wmltxt"))
        XCTAssertFalse(MobileFileTypePolicy.shared.isSelfContainedArchive("book.wmltxt"))
    }

    func testWmltxtResolverNormalizesEntriesAndRejectsEscape() throws {
        let manifest = Data("#!WMLViewer2 ListedFile\n# comment\nchapter\\001.png\n./chapter/002.png\n".utf8)
        XCTAssertEqual(
            try WmltxtEntryResolver.paths(from: manifest),
            ["chapter/001.png", "chapter/002.png"]
        )
        let root = URL(fileURLWithPath: "/provider/book", isDirectory: true)
        XCTAssertEqual(
            try WmltxtEntryResolver.resolve("chapter/001.png", under: root).path,
            "/provider/book/chapter/001.png"
        )
        XCTAssertThrowsError(try WmltxtEntryResolver.paths(
            from: Data("#!WMLViewer2 ListedFile\n../outside.png\n".utf8)
        ))
        XCTAssertThrowsError(try WmltxtEntryResolver.resolve("../outside.png", under: root))
        XCTAssertThrowsError(try WmltxtEntryResolver.resolve("/absolute.png", under: root))
    }

    func testBookmarkRecordManifestFieldsRemainBackwardCompatible() throws {
        let sourceID = UUID()
        let legacy = Data("{\"sourceID\":\"\(sourceID.uuidString)\",\"bookmark\":\"AQID\",\"displayName\":\"Book\",\"isFolder\":true,\"opaqueEntryID\":\"entry\",\"logicalPageIndex\":2}".utf8)
        let restored = try JSONDecoder().decode(BookmarkRecord.self, from: legacy)
        XCTAssertNil(restored.listedManifestOpaqueEntryID)
        XCTAssertNil(restored.listedManifestFileName)
        XCTAssertEqual(restored.logicalPageIndex, 2)
    }

    func testFolderEntryMatcherPrefersOpaqueProviderIdentityThenFileName() {
        let pages = [
            PageItem(id: "opaque-a", url: URL(fileURLWithPath: "/ignored/001.png"), displayName: "001.png", isArchive: false),
            PageItem(id: "opaque-b", url: URL(fileURLWithPath: "/ignored/002.png"), displayName: "002.png", isArchive: false)
        ]
        XCTAssertEqual(DocumentEntryMatcher.index(
            selectedOpaqueEntryID: "opaque-b", selectedFileName: "001.png", in: pages
        ), 1)
        XCTAssertEqual(DocumentEntryMatcher.index(
            selectedOpaqueEntryID: "provider-changed", selectedFileName: "002.PNG", in: pages
        ), 1)
        XCTAssertNil(DocumentEntryMatcher.index(
            selectedOpaqueEntryID: nil, selectedFileName: "missing.png", in: pages
        ))
    }

    func testOpaqueEntryIdentityIsStableAndDoesNotExposeFilesystemPath() {
        let url = URL(fileURLWithPath: "/private/provider/secret/example.png")
        let first = DocumentEntryIdentity.opaqueIdentifier(for: url)
        XCTAssertEqual(first, DocumentEntryIdentity.opaqueIdentifier(for: url))
        XCTAssertFalse(first.contains("private"))
        XCTAssertFalse(first.contains("example.png"))
    }

    func testLocalRenameKeepsResourceIdentityWhenProviderVendsOne() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("wml2viewer-identity-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let original = root.appendingPathComponent("before.png")
        let renamed = root.appendingPathComponent("after.png")
        try Data([0]).write(to: original)
        let before = DocumentEntryIdentity.opaqueIdentifier(for: original)
        try FileManager.default.moveItem(at: original, to: renamed)
        let after = DocumentEntryIdentity.opaqueIdentifier(for: renamed)
        let hasResourceID = (try? renamed.resourceValues(forKeys: [.fileResourceIdentifierKey]).fileResourceIdentifier) != nil
        if hasResourceID {
            XCTAssertEqual(before, after)
        } else {
            XCTAssertNotEqual(before, after)
        }
    }

    func testWideIPadPinsConfiguredFilmstripButPhoneAndNarrowPadUseSheet() {
        XCTAssertTrue(ViewerResponsiveLayout.pinsFilmstrip(isPad: true, width: 1_024, enabled: true))
        XCTAssertFalse(ViewerResponsiveLayout.pinsFilmstrip(isPad: true, width: 899, enabled: true))
        XCTAssertFalse(ViewerResponsiveLayout.pinsFilmstrip(isPad: false, width: 1_024, enabled: true))
        XCTAssertFalse(ViewerResponsiveLayout.pinsFilmstrip(isPad: true, width: 1_024, enabled: false))
    }

    func testCodecRoutingProducesStrictFallbackOrder() {
        XCTAssertEqual(ImageIOCodecRouter.decodeOrder(routing: "DEFAULT"), [.internalCodec, .imageIO])
        XCTAssertEqual(ImageIOCodecRouter.decodeOrder(routing: "INTERNAL_FIRST"), [.internalCodec, .imageIO])
        XCTAssertEqual(ImageIOCodecRouter.decodeOrder(routing: "OS_FIRST"), [.imageIO, .internalCodec])
        XCTAssertEqual(ImageIOCodecRouter.decodeOrder(routing: "INTERNAL_ONLY"), [.internalCodec])
        XCTAssertEqual(ImageIOCodecRouter.decodeOrder(routing: "OS_ONLY"), [.imageIO])
        XCTAssertEqual(ImageIOCodecRouter.decodeOrder(routing: "UNKNOWN"), [.internalCodec, .imageIO])
    }

    func testNativeDecoderCapabilitiesIncludeRetroFormats() throws {
        let extensions = try NativeBridge.internalDecoderExtensions()
        for expected in ["dib", "mag", "mki", "pcd", "pi", "pic", "vsp"] {
            XCTAssertTrue(extensions.contains(expected), "missing native decoder: \(expected)")
        }
    }

    func testMobileFileTypePolicyMatchesAndroidSupportedSet() throws {
        let osExtensions: Set<String> = ["avif", "dng", "heic", "heif"]
        let policy = MobileFileTypePolicy(
            internalImageExtensions: try NativeBridge.internalDecoderExtensions(),
            imageIOImageExtensions: osExtensions
        )
        let androidExtensions: Set<String> = [
            "avif", "bmp", "dib", "dng", "gif", "heic", "heif", "ico", "jpe", "jpeg",
            "jpg", "mag", "mki", "pcd", "pi", "pic", "png", "tif", "tiff", "vsp", "webp",
        ]
        XCTAssertEqual(policy.imageExtensions, androidExtensions)
        for ext in androidExtensions {
            XCTAssertTrue(policy.isImage("PAGE.\(ext.uppercased())"))
        }
        for ext in ["jxl", "pnm", "ppm", "qoi", "svg", "tga"] {
            XCTAssertFalse(policy.isImage("unsupported.\(ext)"))
        }
        XCTAssertEqual(policy.archiveFormat(for: "book.cbz"), "zip")
        XCTAssertEqual(policy.archiveFormat(for: "book.LZH"), "lzh")
        XCTAssertEqual(policy.mimeType(for: "page.jpe"), "image/jpeg")
        XCTAssertNil(policy.mimeType(for: "page.mag"))
    }

    func testLegacyCodecRoutingKeepsInternalFallbackContract() {
        let policy = MobileFileTypePolicy(
            internalImageExtensions: ["mag"], imageIOImageExtensions: []
        )
        XCTAssertTrue(policy.isImage("page.mag"))
        XCTAssertNil(policy.mimeType(for: "page.mag"))
        XCTAssertEqual(ImageIOCodecRouter.decodeOrder(routing: "DEFAULT"), [.internalCodec, .imageIO])
        XCTAssertEqual(ImageIOCodecRouter.decodeOrder(routing: "INTERNAL_FIRST"), [.internalCodec, .imageIO])
        XCTAssertEqual(ImageIOCodecRouter.decodeOrder(routing: "OS_FIRST"), [.imageIO, .internalCodec])
        XCTAssertEqual(ImageIOCodecRouter.decodeOrder(routing: "INTERNAL_ONLY"), [.internalCodec])
        XCTAssertEqual(ImageIOCodecRouter.decodeOrder(routing: "OS_ONLY"), [.imageIO])
    }

    func testCoordinatedFolderSourceListsOnlySupportedDirectChildrenAndReadsSelection() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("wml2viewer-source-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let selected = root.appendingPathComponent("002.png")
        try Data([1, 2, 3]).write(to: selected)
        try Data([4]).write(to: root.appendingPathComponent("001.jpg"))
        try Data([6]).write(to: root.appendingPathComponent("003.mag"))
        try Data([5]).write(to: root.appendingPathComponent("notes.txt"))
        try FileManager.default.createDirectory(
            at: root.appendingPathComponent("subfolder", isDirectory: true),
            withIntermediateDirectories: true
        )

        let source = SecurityScopedDocumentSource(
            sourceID: UUID(), displayName: "fixture", rootURL: root, isFolder: true
        )
        let snapshot = try await source.snapshot()
        let items = snapshot.entries
        XCTAssertEqual(snapshot.enumeratedItemCount, 5)
        XCTAssertEqual(snapshot.supportedItemCount, 3)
        XCTAssertEqual(items.map(\.displayName), ["001.jpg", "002.png", "003.mag"])
        XCTAssertEqual(DocumentEntryMatcher.index(
            selectedOpaqueEntryID: DocumentEntryIdentity.opaqueIdentifier(for: selected),
            selectedFileName: selected.lastPathComponent,
            in: items
        ), 1)
        let selectedData = try await source.read(items[1])
        XCTAssertEqual(selectedData, Data([1, 2, 3]))
        let stat = try await source.stat(items[1])
        XCTAssertEqual(stat.displayName, "002.png")
        XCTAssertEqual(stat.byteSize, 3)
        let materialized = try await source.materialize(items[1])
        defer { try? FileManager.default.removeItem(at: materialized) }
        XCTAssertEqual(try Data(contentsOf: materialized), selectedData)
        let thumbnail = try await source.thumbnail(items[1], maximumPixelSize: 64)
        XCTAssertNil(thumbnail)
    }

    func testMaterializeCacheEvictsLeastRecentlyUsedEntry() async throws {
        let cache = MaterializeCache(limitBytes: 8)
        let first = try await cache.materialize(Data([1, 2, 3, 4]), suggestedExtension: "bin")
        let second = try await cache.materialize(Data([5, 6, 7, 8]), suggestedExtension: "bin")
        await cache.touch(first)
        let third = try await cache.materialize(Data([9, 10, 11, 12]), suggestedExtension: "bin")
        defer {
            try? FileManager.default.removeItem(at: first)
            try? FileManager.default.removeItem(at: second)
            try? FileManager.default.removeItem(at: third)
        }
        #if DEBUG
        let cachedBytes = await cache.cachedByteCount
        let cachedFiles = await cache.cachedFileCount
        XCTAssertEqual(cachedBytes, 8)
        XCTAssertEqual(cachedFiles, 2)
        #endif
        XCTAssertTrue(FileManager.default.fileExists(atPath: first.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: second.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: third.path))
    }

    func testDoubleTapFitOverrideAlternatesWithoutChangingConfiguredFit() {
        let configured = DisplayFit.width
        let first = FitOverridePolicy.next(current: configured)
        XCTAssertEqual(first, .original)
        XCTAssertEqual(FitOverridePolicy.next(current: first), .contain)
        XCTAssertEqual(configured, .width)
    }

    func testNativeReadingPlannerMatchesAndroidRTLSpreadContract() {
        let pages = (0..<6).map { NativeReadingPage(sourceID: 7, portrait: true, cover: $0 == 0) }
        let plan = NativeReadingPlanner.plan(
            pages: pages, currentIndex: 2, landscape: true, layout: .auto,
            direction: .rightToLeft, coverAlone: true, maximumPrefetchSpreads: 2
        )
        XCTAssertEqual(plan?.logicalIndices, [1, 2])
        XCTAssertEqual(plan?.visualIndices, [2, 1])
        XCTAssertEqual(plan?.anchorIndex, 1)
        XCTAssertEqual(plan?.previousAnchorIndex, 0)
        XCTAssertEqual(plan?.nextAnchorIndex, 3)
        XCTAssertEqual(plan?.preloadIndices, [3, 4, 5])
    }

    func testReadingWireRejectsMalformedCountsAndIndices() {
        XCTAssertNil(NativeReadingPlanner.decode([1, 8, 0, -1, -1, 3, 3, 0], pageCount: 1, currentIndex: 0, maximumPrefetchSpreads: 1))
        XCTAssertNil(NativeReadingPlanner.decode([1, 10, 0, -1, -1, 1, 1, 0, 0, 2], pageCount: 1, currentIndex: 0, maximumPrefetchSpreads: 1))
    }

    func testExternalReconcileKeepsCurrentOrSelectsNearestSortedIndex() {
        XCTAssertEqual(ExternalPageReconciler.index(oldIndex: 1, oldID: "b", refreshedIDs: ["a", "b", "c"]), 1)
        XCTAssertEqual(ExternalPageReconciler.index(oldIndex: 2, oldID: "c", refreshedIDs: ["a", "b"]), 1)
        XCTAssertEqual(ExternalPageReconciler.index(oldIndex: 0, oldID: "a", refreshedIDs: ["b", "c"]), 0)
        XCTAssertNil(ExternalPageReconciler.index(oldIndex: 0, oldID: "a", refreshedIDs: []))
    }

    @MainActor
    func testViewerStoreConnectsRTLSpreadPlannerToCurrentPages() {
        let store = ViewerStore()
        store.installTestPages(count: 5)
        XCTAssertEqual(store.testReadingPlan?.logicalIndices, [1, 2])
        XCTAssertEqual(store.testReadingPlan?.visualIndices, [2, 1])
    }

    @MainActor
    func testViewerStorePreviousNextChangesFolderIndex() {
        let store = ViewerStore()
        store.installTestPages(count: 3)
        XCTAssertEqual(store.currentIndex, 1)
        store.next()
        XCTAssertEqual(store.currentIndex, 2)
        store.previous()
        XCTAssertEqual(store.currentIndex, 1)
    }

    @MainActor
    func testFolderPickerIsQueuedUntilPrimaryPickerDismisses() async {
        let store = ViewerStore()
        let selected = URL(fileURLWithPath: "/provider/folder/002.png")
        let primaryID = try! XCTUnwrap(store.installQueuedFolderPickerForTest(selectedURL: selected))

        XCTAssertNil(store.pendingPicker)
        XCTAssertTrue(store.isPickerPresented)
        XCTAssertEqual(store.filesOpenPhase, .processing(.openTarget))

        store.pickerDidDismiss(primaryID)
        await Task.yield()
        XCTAssertEqual(store.pendingPicker?.request, .containingFolder)
        XCTAssertEqual(
            store.pendingPicker?.initialDirectoryURL,
            selected.deletingLastPathComponent()
        )
        XCTAssertTrue(store.isPickerPresented)
    }

    @MainActor
    func testFolderPickerCancellationKeepsSingleSourceInteractive() {
        let store = ViewerStore()
        let selected = URL(fileURLWithPath: "/provider/folder/002.png")
        let presentation = try! XCTUnwrap(
            store.installPendingFolderCancellationForTest(selectedURL: selected)
        )
        let originalPages = store.pages

        store.finishPicker(presentation, nil)

        XCTAssertEqual(store.pages, originalPages)
        XCTAssertTrue(store.touchReady)
        XCTAssertEqual(store.errorMessage, DocumentSourceError.folderRequired.localizedDescription)
        XCTAssertEqual(store.sourceConnectionState, .retryableError)
        store.pickerDidDismiss(presentation.id)
        XCTAssertEqual(store.filesOpenPhase, .idle)
    }
}
