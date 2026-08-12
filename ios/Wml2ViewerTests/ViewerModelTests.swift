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
}
