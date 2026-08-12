import XCTest

final class Wml2ViewerUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    override func tearDownWithError() throws {
        XCUIDevice.shared.orientation = .portrait
        XCUIApplication().terminate()
    }

    func testColdLaunchAndSettingsPanel() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_LANGUAGE"] = "en"
        app.launch()

        XCTAssertTrue(app.buttons["viewer.open"].waitForExistence(timeout: 10))
        XCTAssertTrue(app.buttons["viewer.settings"].exists)
        let emptyState = app.staticTexts.matching(NSPredicate(format: "label == 'No document' OR label == '書類がありません'"))
        XCTAssertTrue(emptyState.firstMatch.waitForExistence(timeout: 5))

        app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        XCTAssertTrue(app.descendants(matching: .any)["settings.panel"].waitForExistence(timeout: 5))
        let done = app.buttons.matching(NSPredicate(format: "label == 'Done' OR label == '完了'"))
        XCTAssertTrue(done.firstMatch.waitForExistence(timeout: 5))
        done.firstMatch.tap()
        XCTAssertTrue(app.buttons["viewer.open"].waitForExistence(timeout: 5))
    }

    func testSystemDocumentBrowserPresentation() throws {
        let app = XCUIApplication()
        app.launch()

        let open = app.buttons["viewer.open"]
        XCTAssertTrue(open.waitForExistence(timeout: 10))
        open.tap()

        let browserPresented = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "isHittable == false"),
            object: open
        )
        XCTAssertEqual(XCTWaiter.wait(for: [browserPresented], timeout: 10), .completed)
    }

    func testRotationKeepsViewerControlsInteractive() throws {
        let app = XCUIApplication()
        app.launch()
        let open = app.buttons["viewer.open"]
        XCTAssertTrue(open.waitForExistence(timeout: 10))

        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(open.waitForExistence(timeout: 5))
        XCTAssertTrue(open.isHittable)

        XCUIDevice.shared.orientation = .portrait
        XCTAssertTrue(open.waitForExistence(timeout: 5))
        XCTAssertTrue(open.isHittable)
    }

    func testCenterZoneOpensSettingsWithoutUsingChromeButton() throws {
        let app = XCUIApplication()
        app.launch()
        XCTAssertTrue(app.buttons["viewer.open"].waitForExistence(timeout: 10))

        app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        XCTAssertTrue(app.descendants(matching: .any)["settings.panel"].waitForExistence(timeout: 5))
    }

    func testBottomCenterZoneOpensFilmstrip() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_HIDE_CHROME"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_LANGUAGE"] = "en"
        app.launch()
        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 10))
        app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.75)).tap()
        XCTAssertTrue(app.descendants(matching: .any)["filmstrip.panel"].waitForExistence(timeout: 5))
    }

    func testLongPressOpensQuickMenuInsteadOfFilmstrip() throws {
        let app = XCUIApplication()
        app.launch()
        XCTAssertTrue(app.buttons["viewer.open"].waitForExistence(timeout: 10))

        app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 0.6)
        XCTAssertTrue(app.buttons["quickMenu.pages"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["filmstrip.list"].exists)
    }

    func testJapaneseSystemLanguageUsesJapaneseStrings() throws {
        let app = XCUIApplication()
        app.launchArguments += ["-AppleLanguages", "(ja)", "-AppleLocale", "ja_JP"]
        app.launchEnvironment["WML2VIEWER_UI_TEST_LANGUAGE"] = "ja"
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launch()

        XCTAssertTrue(app.staticTexts["書類がありません"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.buttons["viewer.settings"].waitForExistence(timeout: 5))
    }
}
