import XCTest
import UIKit

final class Wml2ViewerUITests: XCTestCase {
    override func setUpWithError() throws {
        continueAfterFailure = false
    }

    override func tearDownWithError() throws {
        XCUIDevice.shared.orientation = .portrait
    }

    func testColdLaunchAndSettingsPanel() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_LANGUAGE"] = "en"
        app.launch()

        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 10))
        XCTAssertFalse(app.buttons["viewer.open"].exists)
        XCTAssertFalse(app.buttons["viewer.settings"].exists)
        let emptyState = app.staticTexts.matching(NSPredicate(format: "label == 'No document' OR label == '書類がありません'"))
        XCTAssertTrue(emptyState.firstMatch.waitForExistence(timeout: 5))

        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        XCTAssertTrue(app.descendants(matching: .any)["settings.panel"].waitForExistence(timeout: 5))
        let done = app.buttons.matching(NSPredicate(format: "label == 'Done' OR label == '完了'"))
        XCTAssertTrue(done.firstMatch.waitForExistence(timeout: 5))
        done.firstMatch.tap()
        XCTAssertTrue(surface.exists)
    }

    func testMixedFileAndFolderPickerPresentation() throws {
        let app = XCUIApplication()
        app.launch()

        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 10))
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)).tap()

        let browserPresented = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "isHittable == false"),
            object: surface
        )
        XCTAssertEqual(XCTWaiter.wait(for: [browserPresented], timeout: 10), .completed)
    }

    func testQuickMenuKeepsDocumentBrowserAsFileManagementAction() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launch()
        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 10))
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 0.6)
        let manageFiles = app.buttons.matching(identifier: "quickMenu.manageFiles").firstMatch
        XCTAssertTrue(manageFiles.waitForExistence(timeout: 5))
        manageFiles.tap()
        let close = app.buttons["documentBrowser.close"]
        XCTAssertTrue(close.waitForExistence(timeout: 10))
        XCTAssertFalse(surface.isHittable)
        close.tap()
        XCTAssertTrue(surface.waitForExistence(timeout: 10))
        XCTAssertTrue(surface.isHittable)
    }

    func testRotationKeepsViewerControlsInteractive() throws {
        let app = XCUIApplication()
        app.launch()
        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 10))

        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(surface.waitForExistence(timeout: 5))
        XCTAssertTrue(surface.isHittable)

        XCUIDevice.shared.orientation = .portrait
        XCTAssertTrue(surface.waitForExistence(timeout: 5))
        XCTAssertTrue(surface.isHittable)
    }

    func testCenterZoneOpensSettingsWithoutUsingChromeButton() throws {
        let app = XCUIApplication()
        app.launch()
        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 10))

        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
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
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.75)).tap()
        XCTAssertTrue(app.descendants(matching: .any)["filmstrip.panel"].waitForExistence(timeout: 5))
    }

    func testLongPressOpensQuickMenuInsteadOfFilmstrip() throws {
        let app = XCUIApplication()
        app.launch()
        XCTAssertTrue(app.otherElements["viewer.touchSurface"].waitForExistence(timeout: 10))

        app.otherElements["viewer.touchSurface"].coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 0.6)
        XCTAssertTrue(app.buttons["quickMenu.pages"].waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["filmstrip.list"].exists)
    }

    func testFolderThreeByThreeLeftRightMovesImages() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_FIXTURE_FOLDER"] = "1"
        app.launch()
        let current = app.descendants(matching: .any)["viewer.currentImage"]
        XCTAssertTrue(current.waitForExistence(timeout: 10))
        XCTAssertEqual(current.label, "page-01.png")
        XCTAssertEqual(app.otherElements["viewer.touchSurface"].value as? String, "1 / 3")
        app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)).tap()
        let next = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'viewer.currentImage' AND label == 'page-02.png'")).firstMatch
        XCTAssertTrue(next.waitForExistence(timeout: 10))
        app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.1, dy: 0.5)).tap()
        let previous = app.descendants(matching: .any).matching(NSPredicate(format: "identifier == 'viewer.currentImage' AND label == 'page-01.png'")).firstMatch
        XCTAssertTrue(previous.waitForExistence(timeout: 10))
        app.otherElements["viewer.touchSurface"]
            .coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.82)).tap()
        XCTAssertTrue(app.descendants(matching: .any)["filmstrip.panel"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.descendants(matching: .any)["filmstrip.thumbnail.0"].waitForExistence(timeout: 10))
    }

    func testJapaneseSystemLanguageUsesJapaneseStrings() throws {
        let app = XCUIApplication()
        app.launchArguments += ["-AppleLanguages", "(ja)", "-AppleLocale", "ja_JP"]
        app.launchEnvironment["WML2VIEWER_UI_TEST_LANGUAGE"] = "ja"
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launch()

        XCTAssertTrue(app.staticTexts["書類がありません"].waitForExistence(timeout: 5))
        app.otherElements["viewer.touchSurface"].coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        let panel = app.descendants(matching: .any)["settings.panel"]
        XCTAssertTrue(panel.waitForExistence(timeout: 5))
        XCTAssertEqual(panel.value as? String, "表示")
    }

    func testWideIPadShowsPinnedFilmstripWithoutOpeningSheet() throws {
        guard UIDevice.current.userInterfaceIdiom == .pad else {
            throw XCTSkip("This responsive-layout case runs on the iPad Simulator destination")
        }
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launch()
        XCTAssertTrue(app.otherElements["viewer.touchSurface"].waitForExistence(timeout: 10))

        XCUIDevice.shared.orientation = .landscapeLeft
        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 5))
        XCTAssertFalse(app.descendants(matching: .any)["filmstrip.panel"].exists)
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.8)).tap()
        XCTAssertTrue(app.descendants(matching: .any)["filmstrip.panel"].waitForExistence(timeout: 5))
        XCTAssertTrue(app.otherElements["viewer.touchSurface"].isHittable)
    }

    func testViewerHasNoPersistentNavigationOrDuplicateOpenIcons() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launch()
        XCTAssertTrue(app.otherElements["viewer.touchSurface"].waitForExistence(timeout: 10))
        for identifier in [
            "viewer.open", "viewer.settings", "viewer.previous", "viewer.next",
            "viewer.folder", "viewer.filmstrip"
        ] {
            XCTAssertFalse(app.buttons[identifier].exists, "Unexpected persistent control: \(identifier)")
        }
    }

    func testUnsupportedImageLeavesThreeByThreeSettingsInteractive() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_FIXTURE_UNSUPPORTED"] = "1"
        app.launch()

        let error = app.descendants(matching: .any)["viewer.error"]
        XCTAssertTrue(error.waitForExistence(timeout: 15))
        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.isHittable)
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)).tap()
        XCTAssertTrue(app.descendants(matching: .any)["settings.panel"].waitForExistence(timeout: 5))
    }

    func testZipArchiveDisplaysItsImageEntry() throws {
        try assertArchiveDisplaysEntry(format: "zip", entryName: "page-01.png", routing: "OS_ONLY")
    }

    func testLzhArchiveDisplaysItsImageEntry() throws {
        let app = try assertArchiveDisplaysEntry(format: "lzh", entryName: "page-01.mag")
        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 5))
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)).tap()
        let second = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier == 'viewer.currentImage' AND label == 'page-02.mag'")
        ).firstMatch
        XCTAssertTrue(second.waitForExistence(timeout: 15), "second MAG entry was not displayed")
    }

    @discardableResult
    private func assertArchiveDisplaysEntry(
        format: String,
        entryName: String,
        routing: String? = nil
    ) throws -> XCUIApplication {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_FIXTURE_ARCHIVE"] = format
        if let routing {
            app.launchEnvironment["WML2VIEWER_UI_TEST_CODEC_ROUTING"] = routing
        }
        app.launch()

        let image = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier == 'viewer.currentImage' AND label == %@", entryName)
        ).firstMatch
        XCTAssertTrue(image.waitForExistence(timeout: 15), "\(format) entry was not displayed")
        XCTAssertFalse(app.descendants(matching: .any)["viewer.error"].exists)
        return app
    }
}
