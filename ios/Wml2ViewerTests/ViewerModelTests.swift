import XCTest
@testable import Wml2Viewer

final class ViewerModelTests: XCTestCase {
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
        XCTAssertTrue(PickerRequest.folder.selectionIsFolder(resourceIsDirectory: false))
        XCTAssertTrue(PickerRequest.file.selectionIsFolder(resourceIsDirectory: true))
        XCTAssertFalse(PickerRequest.file.selectionIsFolder(resourceIsDirectory: false))
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

    func testCoordinatedFolderSourceListsOnlySupportedDirectChildrenAndReadsSelection() async throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("wml2viewer-source-\(UUID().uuidString)", isDirectory: true)
        defer { try? FileManager.default.removeItem(at: root) }
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        let selected = root.appendingPathComponent("002.png")
        try Data([1, 2, 3]).write(to: selected)
        try Data([4]).write(to: root.appendingPathComponent("001.jpg"))
        try Data([5]).write(to: root.appendingPathComponent("notes.txt"))
        try FileManager.default.createDirectory(
            at: root.appendingPathComponent("subfolder", isDirectory: true),
            withIntermediateDirectories: true
        )

        let source = SecurityScopedDocumentSource(
            sourceID: UUID(), displayName: "fixture", rootURL: root, isFolder: true
        )
        let items = try await source.list()
        XCTAssertEqual(items.map(\.displayName), ["001.jpg", "002.png"])
        let selectedData = try await source.read(items[1])
        XCTAssertEqual(selectedData, Data([1, 2, 3]))
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
}
