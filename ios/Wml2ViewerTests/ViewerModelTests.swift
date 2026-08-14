import XCTest
import UniformTypeIdentifiers
@testable import Wml2Viewer

final class ViewerModelTests: XCTestCase {
    @MainActor
    func testPlaceholderFolderUsesDeclaredFolderTypeWhenDirectoryFlagIsMissing() {
        XCTAssertTrue(
            ViewerStore.isDirectoryResource(
                isDirectory: nil,
                contentType: .folder,
                hasDirectoryPath: false
            )
        )
        XCTAssertFalse(
            ViewerStore.isDirectoryResource(
                isDirectory: nil,
                contentType: .png,
                hasDirectoryPath: false
            )
        )
    }

    func testProviderErrorsBecomeActionableLocalizedStates() {
        let auth = NSError(domain: "NSFileProviderErrorDomain", code: -1000)
        XCTAssertEqual(
            DocumentSourceError.normalized(auth) as? DocumentSourceError,
            .providerAuthenticationRequired
        )
        let offline = NSError(domain: NSURLErrorDomain, code: NSURLErrorNotConnectedToInternet)
        XCTAssertEqual(
            DocumentSourceError.normalized(offline) as? DocumentSourceError,
            .providerOffline
        )
        let unavailable = NSError(domain: "NSFileProviderErrorDomain", code: -2012)
        XCTAssertEqual(
            DocumentSourceError.normalized(unavailable) as? DocumentSourceError,
            .providerUnavailable
        )
        let existing = DocumentSourceError.noSupportedItems
        XCTAssertTrue(DocumentSourceError.normalized(existing) is DocumentSourceError)
    }

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
        report.recordPickerRequested()
        report.recordRecoverableError(inputReady: true)
        XCTAssertTrue(report.recoverableErrorObserved)
        XCTAssertTrue(report.inputReadyAfterError)
        XCTAssertFalse(report.recoveredAfterError)

        report.recordDecodeReady(pageCount: 2)
        XCTAssertTrue(report.recoveredAfterError)
        XCTAssertEqual(report.status, "in-progress")
    }

    func testProviderAcceptanceIgnoresRestoredSourceBeforeFilesIsRequested() {
        var report = ProviderAcceptanceReport(token: "fresh-token", provider: .local)

        // These are the exact events emitted when the app restores an old
        // single-file bookmark before the operator touches the Files entry.
        report.recordDecodeReady(pageCount: 1)
        report.recordNavigation(from: 0, to: 0)
        report.recordFilmstripOpened()
        report.recordThumbnailDecoded()

        XCTAssertEqual(report.sequence, 0)
        XCTAssertEqual(report.decodedPageCount, 0)
        XCTAssertFalse(report.filmstripOpened)
        XCTAssertFalse(report.thumbnailDecoded)
        XCTAssertEqual(report.status, "in-progress")

        report.recordPickerRequested()
        report.recordFolderSnapshot(enumerated: 3, supported: 3)
        report.recordDecodeReady(pageCount: 3)
        report.recordNavigation(from: 0, to: 1)
        report.recordNavigation(from: 1, to: 0)
        report.recordFilmstripOpened()
        report.recordThumbnailDecoded()
        XCTAssertEqual(report.status, "passed")
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

    func testTouchMapDefaultsAreConfigurableAndPersisted() throws {
        let center = TouchZone(row: 1, column: 1)
        var config = MobileConfigV1()
        XCTAssertEqual(config.touchMap.action(for: center), .settings)
        config.touchMap = config.touchMap.setting(zone: center, action: .openContextMenu)
        XCTAssertEqual(config.touchMap.action(for: center), .openContextMenu)

        let restored = try JSONDecoder().decode(
            MobileConfigV1.self,
            from: JSONEncoder().encode(config)
        )
        XCTAssertEqual(restored.touchMap.action(for: center), .openContextMenu)
        XCTAssertEqual(restored.doubleTapAction, .toggleFitMode)
        XCTAssertEqual(restored.longPressAction, .openContextMenu)
    }

    func testLegacyConfigGetsSafeGestureActionDefaults() throws {
        let legacy = try JSONDecoder().decode(
            MobileConfigV1.self,
            from: Data(#"{"schemaVersion":1,"touchZonesEnabled":true}"#.utf8)
        )
        XCTAssertEqual(legacy.touchMap.action(for: TouchZone(row: 0, column: 1)), .openFiler)
        XCTAssertEqual(legacy.doubleTapAction, .toggleFitMode)
        XCTAssertEqual(legacy.longPressAction, .openContextMenu)
    }

    @MainActor
    func testViewerActionDispatcherSupportsSafeNavigationAndRuntimeControls() {
        let store = ViewerStore()
        store.installTestPages(count: 3)
        store.perform(.last)
        XCTAssertEqual(store.currentIndex, 2)
        store.perform(.first)
        XCTAssertEqual(store.currentIndex, 0)
        store.perform(.zoomIn)
        XCTAssertEqual(store.zoom, 1.25, accuracy: 0.001)
        store.perform(.zoomReset)
        XCTAssertEqual(store.zoom, 1, accuracy: 0.001)
        store.perform(.toggleFitMode)
        XCTAssertEqual(store.runtimeFit, .original)
        store.perform(.toggleFitMode)
        XCTAssertEqual(store.runtimeFit, .contain)
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

    func testFilesOpenFlowQueuesFollowUpAfterDismissalWithoutAcceptingStaleDisappear() {
        var flow = FilesOpenFlowMachine()
        let primary = try! XCTUnwrap(flow.begin(.openTarget))
        XCTAssertTrue(flow.beginResultProcessing(primary))
        XCTAssertNil(flow.didDismiss(primary.id))

        let folder = try! XCTUnwrap(flow.queueFollowUp(
            .containingFolder,
            initialDirectoryURL: URL(fileURLWithPath: "/provider/folder")
        ))
        XCTAssertEqual(flow.phase, .presenting(.containingFolder))
        XCTAssertFalse(flow.activePresentationWasDismissed)

        // A delayed disappearance callback from the primary picker must not
        // dismiss the newly presented folder picker.
        XCTAssertNil(flow.didDismiss(primary.id))
        XCTAssertEqual(flow.activePresentationID, folder.id)
        XCTAssertFalse(flow.activePresentationWasDismissed)
        XCTAssertTrue(flow.beginResultProcessing(folder))
    }

    func testFilesOpenFlowRejectsDuplicateContainingFolderFollowUp() {
        var flow = FilesOpenFlowMachine()
        let primary = try! XCTUnwrap(flow.begin(.openTarget))
        XCTAssertTrue(flow.beginResultProcessing(primary))

        XCTAssertNil(flow.queueFollowUp(
            .containingFolder,
            initialDirectoryURL: URL(fileURLWithPath: "/provider/folder")
        ))
        XCTAssertNil(flow.queueFollowUp(
            .containingFolder,
            initialDirectoryURL: URL(fileURLWithPath: "/provider/folder")
        ))

        let folder = try! XCTUnwrap(flow.didDismiss(primary.id))
        XCTAssertTrue(flow.beginResultProcessing(folder))
        XCTAssertNil(flow.didDismiss(folder.id), "a duplicate callback must not queue a third picker")
    }

    func testFolderPickerGuidanceExplainsAutomaticSecondPicker() {
        let selectedFileFollowUp = PickerPresentation(
            id: UUID(),
            flowID: UUID(),
            request: .containingFolder,
            initialDirectoryURL: URL(fileURLWithPath: "/provider/folder")
        )
        let standaloneFolderPicker = PickerPresentation(
            id: UUID(),
            flowID: UUID(),
            request: .containingFolder,
            initialDirectoryURL: nil
        )
        let primary = PickerPresentation(
            id: UUID(),
            flowID: UUID(),
            request: .openTarget,
            initialDirectoryURL: nil
        )

        let followUpMessage = PickerFolderGuidance.message(for: selectedFileFollowUp)
        let standaloneMessage = PickerFolderGuidance.message(for: standaloneFolderPicker)
        XCTAssertNotNil(followUpMessage)
        XCTAssertNotNil(standaloneMessage)
        XCTAssertNotEqual(followUpMessage, standaloneMessage)
        XCTAssertNil(PickerFolderGuidance.message(for: primary))
    }

    func testFilesOpenFlowPresentsArchiveWithoutContainingFolderFollowUp() {
        for name in ["book.zip", "book.lha", "book.lzh"] {
            XCTAssertTrue(MobileFileTypePolicy.shared.isSelfContainedArchive(name))
            XCTAssertFalse(ContainingFolderAuthorizationPolicy.shouldRequest(
                isFolder: false,
                isSupported: MobileFileTypePolicy.shared.isSupported(name),
                isSelfContainedArchive: MobileFileTypePolicy.shared.isSelfContainedArchive(name)
            ))
        }
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

    func testSelectedDocumentPolicySeparatesSupportedFileFromUnsupportedExtension() throws {
        XCTAssertNoThrow(try SelectedDocumentPolicy.validate(name: "page.png", isFolder: false))
        XCTAssertTrue(SelectedDocumentPolicy.isSupported(url: URL(fileURLWithPath: "page.png")))
        XCTAssertNoThrow(try SelectedDocumentPolicy.validate(name: "book.LZH", isFolder: false))
        XCTAssertNoThrow(
            try SelectedDocumentPolicy.validate(
                name: "extensionless-item", isFolder: false, declaredMime: "image/png"
            )
        )
        XCTAssertNoThrow(try SelectedDocumentPolicy.validate(name: "Any Folder", isFolder: true))
        XCTAssertThrowsError(
            try SelectedDocumentPolicy.validate(name: "manual.pdf", isFolder: false)
        ) { error in
            guard let documentError = error as? DocumentSourceError,
                  case .unsupportedFileType(let fileExtension) = documentError else {
                return XCTFail("unexpected error: \(error)")
            }
            XCTAssertEqual(fileExtension, "pdf")
            XCTAssertTrue(error.localizedDescription.localizedCaseInsensitiveContains("PDF"))
            XCTAssertTrue(error.localizedDescription.contains("ZIP/LHA/LZH"))
        }
    }

    func testSourceOpeningProgressReportsCountAndCancellationStopsSnapshot() async throws {
        let progress = SourceOpeningProgress(
            isFolder: true,
            processedItemCount: 32,
            totalItemCount: 100
        )
        XCTAssertTrue(progress.detail.contains("32"))
        XCTAssertTrue(progress.detail.contains("100"))

        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(".test-wml2viewer-cancel-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try Data([0]).write(to: root.appendingPathComponent("page.png"))
        let source = SecurityScopedDocumentSource(
            sourceID: UUID(), displayName: "cancel fixture", rootURL: root, isFolder: true
        )
        let cancellation = DocumentSourceCancellation()
        cancellation.cancel()
        do {
            _ = try await source.snapshot(cancellation: cancellation)
            XCTFail("cancelled snapshot unexpectedly succeeded")
        } catch is CancellationError {
            XCTAssertTrue(cancellation.isCancelled)
        }
    }

    func testSingleFileSnapshotRetainsTheSecurityScopedRootURL() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(".test-wml2viewer-single-\(UUID().uuidString).png")
        defer { try? FileManager.default.removeItem(at: root) }
        try Data([0]).write(to: root)
        let source = SecurityScopedDocumentSource(
            sourceID: UUID(), displayName: root.lastPathComponent,
            rootURL: root, isFolder: false
        )

        let snapshot = try await source.snapshot()

        XCTAssertEqual(snapshot.entries.count, 1)
        XCTAssertEqual(snapshot.entries[0].url, root)
        XCTAssertEqual(snapshot.entries[0].displayName, root.lastPathComponent)
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

    func testImageIOCapabilityProbeOnlyReturnsCandidateTypes() {
        let probed = ImageIOCodecRouter.capabilityProbe()
        XCTAssertTrue(probed.isSubset(of: ImageIOCodecRouter.mobileProbeCandidates))
        XCTAssertEqual(probed, ImageIOCodecRouter.supportedImageExtensions)
    }

    func testImageIODetectsAnimatedContainerWhenOSReturnsPoster() {
        XCTAssertTrue(ImageIOCodecRouter.encodedAnimationHint(
            Data([0x47, 0x49, 0x46, 0x38, 0x39, 0x61, 0x2c, 0x00, 0x2c])
        ))
        XCTAssertTrue(ImageIOCodecRouter.encodedAnimationHint(
            Data([0x89, 0x50, 0x4e, 0x47]) + Data("acTL".utf8)
        ))
        XCTAssertTrue(ImageIOCodecRouter.encodedAnimationHint(
            Data("RIFFxxxxWEBPANMF".utf8)
        ))
        XCTAssertFalse(ImageIOCodecRouter.encodedAnimationHint(Data("static".utf8)))
    }

    func testAvailableExportFormatsAreImageIODecodable() throws {
        XCTAssertTrue(ImageIOCodecRouter.availableExportFormats.contains(.png))
        XCTAssertTrue(ImageIOCodecRouter.availableExportFormats.contains(.jpeg))
        XCTAssertTrue(ImageIOCodecRouter.availableExportFormats.allSatisfy {
            ImageIOCodecRouter.mobileProbeCandidates.contains($0.fileExtension)
                || $0.fileExtension == "jpg"
        })
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

    func testAutomaticMaterializeLimitPreservesReserveAndBounds() {
        let oneGiB = Int64(1_024 * 1_024 * 1_024)
        XCTAssertEqual(
            MaterializeCache.automaticLimitBytes(freeBytes: 10 * oneGiB),
            Int(1 * oneGiB)
        )
        XCTAssertEqual(
            MaterializeCache.automaticLimitBytes(freeBytes: 100 * oneGiB),
            Int(2 * oneGiB)
        )
        XCTAssertEqual(
            MaterializeCache.automaticLimitBytes(freeBytes: 2 * oneGiB),
            256 * 1_048_576
        )
    }

    func testPinnedMaterializedEntryIsNotEvictedUntilUnpinned() async throws {
        let cache = MaterializeCache(limitBytes: 8)
        let first = try await cache.materialize(Data([1, 2, 3, 4]), suggestedExtension: "bin")
        await cache.pin(first)
        let second = try await cache.materialize(Data([5, 6, 7, 8]), suggestedExtension: "bin")
        let third = try await cache.materialize(Data([9, 10, 11, 12]), suggestedExtension: "bin")
        XCTAssertTrue(FileManager.default.fileExists(atPath: first.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: second.path))
        XCTAssertTrue(FileManager.default.fileExists(atPath: third.path))
        await cache.unpin(first)
        try? FileManager.default.removeItem(at: first)
        try? FileManager.default.removeItem(at: third)
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

    func testFailedFolderPageSkipPreservesForwardAndBackwardDirection() {
        let failed: Set<Int> = [1, 2]
        XCTAssertEqual(
            FolderPageFailureNavigator.replacementIndex(
                failedIndex: 1,
                pageCount: 5,
                failedIndices: failed,
                direction: .forward
            ),
            3
        )
        XCTAssertEqual(
            FolderPageFailureNavigator.replacementIndex(
                failedIndex: 2,
                pageCount: 5,
                failedIndices: failed,
                direction: .backward
            ),
            0
        )
    }

    func testFailedFolderPageAtBoundaryFallsBackWithoutWrapping() {
        XCTAssertEqual(
            FolderPageFailureNavigator.replacementIndex(
                failedIndex: 0,
                pageCount: 4,
                failedIndices: [0],
                direction: .backward
            ),
            1
        )
        XCTAssertEqual(
            FolderPageFailureNavigator.replacementIndex(
                failedIndex: 3,
                pageCount: 4,
                failedIndices: [3],
                direction: .forward
            ),
            2
        )
        XCTAssertNil(
            FolderPageFailureNavigator.replacementIndex(
                failedIndex: 0,
                pageCount: 2,
                failedIndices: [0, 1],
                direction: .backward
            )
        )
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
    func testViewerNavigationClearsPriorFailureMessages() {
        let store = ViewerStore()
        store.installTestPages(count: 3)
        store.errorMessage = "failed page"
        store.sourceNoticeMessage = "skipped page"

        store.previous()

        XCTAssertNil(store.errorMessage)
        XCTAssertNil(store.sourceNoticeMessage)
        XCTAssertEqual(store.currentIndex, 0)
    }

    @MainActor
    func testSceneBackgroundStopsDeferredWorkAndActivationRestoresIt() {
        let store = ViewerStore()
        store.handleScenePhase(.background)
        XCTAssertEqual(store.currentScenePhase, .background)

        store.handleScenePhase(.active)
        XCTAssertEqual(store.currentScenePhase, .active)
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
        XCTAssertNil(store.errorMessage)
        XCTAssertEqual(
            store.sourceNoticeMessage,
            String(localized: "Folder selection was cancelled. The selected file remains open by itself.")
        )
        store.pickerDidDismiss(presentation.id)
        XCTAssertEqual(store.filesOpenPhase, .idle)
    }

    @MainActor
    func testPickerCrashDismissalReturnsToViewerAndPreservesSource() {
        let store = ViewerStore()
        let presentation = try! XCTUnwrap(store.installPendingPickerForTest())
        let originalPages = store.pages

        store.pickerDidDismiss(presentation.id)

        XCTAssertNil(store.pendingPicker)
        XCTAssertEqual(store.pages, originalPages)
        XCTAssertEqual(store.filesOpenPhase, .idle)
        XCTAssertTrue(store.touchReady)
        XCTAssertEqual(
            store.sourceNoticeMessage,
            String(localized: "Files closed unexpectedly. The current document remains open.")
        )
    }

    @MainActor
    func testRepeatedUnexpectedPickerDismissalsEnterRecoveryGuard() {
        let store = ViewerStore()

        for _ in 0..<3 {
            let presentation = try! XCTUnwrap(store.installPendingPickerForTest())
            store.pickerDidDismiss(presentation.id)
        }

        XCTAssertTrue(store.filesPickerRecoveryRequired)
        store.requestFilePicker()
        XCTAssertNil(store.pendingPicker)
        XCTAssertEqual(store.filesOpenPhase, .idle)

        store.resetFilesRecovery()
        XCTAssertFalse(store.filesPickerRecoveryRequired)
        XCTAssertNil(store.sourceNoticeMessage)
    }

    func testBookmarkStoreClearRemovesOnlyAppOwnedRecords() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent(".test-bookmark-clear-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = BookmarkStore(fileURL: directory.appendingPathComponent("bookmarks.json"))
        try await store.upsert(
            BookmarkRecord(
                sourceID: UUID(), bookmark: Data([1, 2, 3]), displayName: "sample",
                isFolder: false, opaqueEntryID: "opaque", logicalPageIndex: 0
            )
        )

        try await store.clear()
        let records = try await store.load()
        XCTAssertTrue(records.isEmpty)
    }

    @MainActor
    func testSceneReturnRecoversPickerWithNoDelegateCallback() async {
        let store = ViewerStore()
        let presentation = try! XCTUnwrap(store.installPendingPickerForTest())

        store.handleScenePhase(.inactive)
        store.handleScenePhase(.active)
        try? await Task.sleep(nanoseconds: 500_000_000)

        XCTAssertNil(store.pendingPicker)
        XCTAssertEqual(store.filesOpenPhase, .idle)
        XCTAssertTrue(store.touchReady)
        XCTAssertEqual(
            store.sourceNoticeMessage,
            String(localized: "Files closed unexpectedly. The current document remains open.")
        )
        XCTAssertNotEqual(store.pendingPicker?.id, presentation.id)
    }

    @MainActor
    func testRestoreLastLocationNeverPresentsFilesPicker() async {
        let store = ViewerStore()
        await store.restoreLastLocation()

        XCTAssertNil(store.pendingPicker)
        XCTAssertFalse(store.filesOpenPhase.blocksViewerInput)
    }

    @MainActor
    func testRestoreLastLocationReopensBookmarkWithoutFilesPicker() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(".test-restore-location-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let page = root.appendingPathComponent("001.png")
        try Data([0]).write(to: page)

        let bookmarkURL = root.appendingPathComponent("bookmarks.json")
        let configURL = root.appendingPathComponent("config.json")
        let bookmarkStore = BookmarkStore(fileURL: bookmarkURL)
        let sourceID = UUID()
        let bookmark = try root.bookmarkData(
            options: [.suitableForBookmarkFile],
            includingResourceValuesForKeys: nil,
            relativeTo: nil
        )
        try await bookmarkStore.upsert(
            BookmarkRecord(
                sourceID: sourceID,
                bookmark: bookmark,
                displayName: root.lastPathComponent,
                isFolder: true,
                opaqueEntryID: nil,
                logicalPageIndex: 0
            )
        )

        let store = ViewerStore(
            bookmarks: bookmarkStore,
            configStore: ConfigStore(fileURL: configURL)
        )
        await store.restoreLastLocation()

        XCTAssertTrue(store.hasRestorableLocation)
        XCTAssertEqual(store.pages.count, 1)
        XCTAssertEqual(store.pages.first?.displayName, "001.png")
        XCTAssertNil(store.pendingPicker)
        XCTAssertFalse(store.filesOpenPhase.blocksViewerInput)

        for _ in 0..<3 {
            let presentation = try XCTUnwrap(store.installPendingPickerForTest())
            store.pickerDidDismiss(presentation.id)
        }
        XCTAssertTrue(store.filesPickerRecoveryRequired)

        await store.restoreLastLocation()
        XCTAssertFalse(store.filesPickerRecoveryRequired)
    }

    func testDocumentBrowserCrashUsesRecoveryClosureOnlyOnce() {
        var pickCalls = 0
        var recoveryCalls = 0
        let coordinator = DocumentBrowserView.Coordinator(
            onPick: { _ in pickCalls += 1 },
            onUnexpectedDismissal: { recoveryCalls += 1 }
        )

        coordinator.notifyUnexpectedDismissalIfNeeded()
        coordinator.notifyUnexpectedDismissalIfNeeded()

        XCTAssertEqual(recoveryCalls, 1)
        XCTAssertEqual(pickCalls, 0)
    }
}
