import XCTest
import UIKit

final class Wml2ViewerUITests: XCTestCase {
    private enum PickerFixture {
        static let folderName = "Picker Continuous Pages"
        static let pageNames = ["page-01.png", "page-02.png", "page-03.png"]
    }

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

    func testOSPickerFolderSelectionConnectsThreePageSource() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_LANGUAGE"] = "en"
        app.launchEnvironment["WML2VIEWER_UI_TEST_PICKER_FOLDER_NAME"] = PickerFixture.folderName
        app.launch()

        XCTAssertTrue(
            app.descendants(matching: .any)["uiTest.pickerFixtureReady"]
                .waitForExistence(timeout: 10),
            "The picker fixture was not seeded"
        )
        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 10))
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)).tap()

        let folder = app.cells.matching(
            NSPredicate(format: "identifier == %@", "\(PickerFixture.folderName), Folder")
        ).firstMatch
        XCTAssertTrue(folder.waitForExistence(timeout: 15), pickerFailureDescription(app))
        folder.tap()

        let firstPage = app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier == 'viewer.currentImage' AND label == %@",
                PickerFixture.pageNames[0]
            )
        ).firstMatch
        if !firstPage.waitForExistence(timeout: 2) {
            let openButton = app.buttons.matching(
                NSPredicate(format: "label == 'Open' OR label == '開く'")
            ).firstMatch
            XCTAssertTrue(openButton.waitForExistence(timeout: 10), pickerFailureDescription(app))
            openButton.tap()
        }

        XCTAssertTrue(firstPage.waitForExistence(timeout: 15), pickerFailureDescription(app))
        XCTAssertEqual(surface.value as? String, "1 / 3")

        app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)).tap()
        let secondPage = app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier == 'viewer.currentImage' AND label == %@",
                PickerFixture.pageNames[1]
            )
        ).firstMatch
        XCTAssertTrue(secondPage.waitForExistence(timeout: 10))

        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.82)).tap()
        XCTAssertTrue(app.descendants(matching: .any)["filmstrip.panel"].waitForExistence(timeout: 5))
        for (index, pageName) in PickerFixture.pageNames.enumerated() {
            XCTAssertTrue(app.buttons.matching(NSPredicate(format: "label CONTAINS %@", pageName)).firstMatch.exists)
            XCTAssertTrue(
                app.descendants(matching: .any)["filmstrip.thumbnail.\(index)"]
                    .waitForExistence(timeout: 10)
            )
        }
    }

    func testOSPickerImageThenContainingFolderPreservesMiddlePage() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_LANGUAGE"] = "en"
        app.launchEnvironment["WML2VIEWER_UI_TEST_PICKER_FOLDER_NAME"] = PickerFixture.folderName
        app.launch()

        XCTAssertTrue(
            app.descendants(matching: .any)["uiTest.pickerFixtureReady"]
                .waitForExistence(timeout: 10)
        )
        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 10))
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)).tap()

        let folder = app.cells.matching(
            NSPredicate(format: "identifier == %@", "\(PickerFixture.folderName), Folder")
        ).firstMatch
        XCTAssertTrue(folder.waitForExistence(timeout: 15), pickerFailureDescription(app))
        folder.tap()

        let selectedName = PickerFixture.pageNames[1]
        let selectedURL = URL(fileURLWithPath: selectedName)
        let pickerIdentifier = "\(selectedURL.deletingPathExtension().lastPathComponent), \(selectedURL.pathExtension)"
        let selectedFile = app.cells.matching(
            NSPredicate(format: "identifier == %@", pickerIdentifier)
        ).firstMatch
        XCTAssertTrue(selectedFile.waitForExistence(timeout: 10), pickerFailureDescription(app))
        selectedFile.tap()

        let selectedImage = app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier == 'viewer.currentImage' AND label == %@",
                selectedName
            )
        ).firstMatch
        if !selectedImage.waitForExistence(timeout: 2) {
            let openSelectedFile = app.buttons.matching(
                NSPredicate(format: "label == 'Open' OR label == '開く'")
            ).firstMatch
            XCTAssertTrue(openSelectedFile.waitForExistence(timeout: 10), pickerFailureDescription(app))
            openSelectedFile.tap()
        }

        XCTAssertTrue(selectedImage.waitForExistence(timeout: 15), pickerFailureDescription(app))
        let singleSource = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == '1 / 1' AND isHittable == false"),
            object: surface
        )
        XCTAssertEqual(
            XCTWaiter.wait(for: [singleSource], timeout: 15), .completed,
            pickerFailureDescription(app)
        )

        let authorizeFolder = app.buttons.matching(
            NSPredicate(format: "label == 'Open' OR label == '開く'")
        ).firstMatch
        XCTAssertTrue(authorizeFolder.waitForExistence(timeout: 10), pickerFailureDescription(app))
        authorizeFolder.tap()

        let promotedSource = XCTNSPredicateExpectation(
            predicate: NSPredicate(format: "value == '2 / 3' AND isHittable == true"),
            object: surface
        )
        XCTAssertEqual(
            XCTWaiter.wait(for: [promotedSource], timeout: 15), .completed,
            pickerFailureDescription(app)
        )
        XCTAssertEqual(selectedImage.label, selectedName)

        app.windows.firstMatch.coordinate(withNormalizedOffset: CGVector(dx: 0.9, dy: 0.5)).tap()
        let lastPage = app.descendants(matching: .any).matching(
            NSPredicate(
                format: "identifier == 'viewer.currentImage' AND label == %@",
                PickerFixture.pageNames[2]
            )
        ).firstMatch
        XCTAssertTrue(lastPage.waitForExistence(timeout: 10))
    }

    func testOSPickerUnsupportedFileExplainsTheRejectedType() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_LANGUAGE"] = "en"
        app.launchEnvironment["WML2VIEWER_UI_TEST_PICKER_FOLDER_NAME"] = PickerFixture.folderName
        app.launchEnvironment["WML2VIEWER_UI_TEST_PICKER_UNSUPPORTED_FILE"] = "1"
        app.launch()

        XCTAssertTrue(app.descendants(matching: .any)["uiTest.pickerFixtureReady"].waitForExistence(timeout: 10))
        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 10))
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)).tap()

        let folder = app.cells.matching(
            NSPredicate(format: "identifier == %@", "\(PickerFixture.folderName), Folder")
        ).firstMatch
        XCTAssertTrue(folder.waitForExistence(timeout: 15), pickerFailureDescription(app))
        folder.tap()
        let unsupported = app.cells.matching(
            NSPredicate(format: "identifier == 'unsupported, pdf'")
        ).firstMatch
        XCTAssertTrue(unsupported.waitForExistence(timeout: 10), pickerFailureDescription(app))
        unsupported.tap()
        let open = app.buttons.matching(
            NSPredicate(format: "label == 'Open' OR label == '開く'")
        ).firstMatch
        if open.waitForExistence(timeout: 2) { open.tap() }

        let error = app.descendants(matching: .any)["viewer.error"]
        XCTAssertTrue(error.waitForExistence(timeout: 15), pickerFailureDescription(app))
        XCTAssertTrue(error.label.localizedCaseInsensitiveContains("PDF"))
        XCTAssertTrue(error.label.localizedCaseInsensitiveContains("ZIP/LHA/LZH"))
        XCTAssertTrue(surface.isHittable)
    }

    func testFolderScanShowsCancellableProgressAndReturnsControl() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_LANGUAGE"] = "en"
        app.launchEnvironment["WML2VIEWER_UI_TEST_PICKER_FOLDER_NAME"] = PickerFixture.folderName
        app.launchEnvironment["WML2VIEWER_UI_TEST_SOURCE_OPENING_DELAY_NANOSECONDS"] = "10000000000"
        app.launch()

        XCTAssertTrue(app.descendants(matching: .any)["uiTest.pickerFixtureReady"].waitForExistence(timeout: 10))
        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 10))
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15)).tap()
        let folder = app.cells.matching(
            NSPredicate(format: "identifier == %@", "\(PickerFixture.folderName), Folder")
        ).firstMatch
        XCTAssertTrue(folder.waitForExistence(timeout: 15), pickerFailureDescription(app))
        folder.tap()
        let open = app.buttons.matching(
            NSPredicate(format: "label == 'Open' OR label == '開く'")
        ).firstMatch
        if open.waitForExistence(timeout: 2) { open.tap() }

        XCTAssertTrue(
            app.descendants(matching: .any)["sourceOpening.progress"].waitForExistence(timeout: 10),
            pickerFailureDescription(app)
        )
        let cancel = app.buttons["sourceOpening.cancel"]
        XCTAssertTrue(cancel.isHittable)
        cancel.tap()
        XCTAssertFalse(app.descendants(matching: .any)["sourceOpening.progress"].waitForExistence(timeout: 2))
        XCTAssertTrue(app.descendants(matching: .any)["viewer.sourceNotice"].waitForExistence(timeout: 5))
        XCTAssertTrue(surface.isHittable)
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

    func testFolderPickerExplainsWhyOpenMustBeTappedAgain() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_LANGUAGE"] = "en"
        app.launch()

        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 10))
        surface.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
            .press(forDuration: 0.6)
        let chooseFolder = app.buttons["quickMenu.folder"]
        XCTAssertTrue(chooseFolder.waitForExistence(timeout: 5))
        chooseFolder.tap()

        let guidance = app.descendants(matching: .any)["documentPicker.folderGuidance"]
        XCTAssertTrue(guidance.waitForExistence(timeout: 10), pickerFailureDescription(app))
        XCTAssertTrue(guidance.label.localizedCaseInsensitiveContains("Open"))
        XCTAssertFalse(surface.isHittable)
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

    func testEmptyArchiveInFolderIsSkippedAndNextImageOpens() throws {
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_FIXTURE_FOLDER"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_FOLDER_EMPTY_ARCHIVE"] = "1"
        app.launch()

        let image = app.descendants(matching: .any).matching(
            NSPredicate(format: "identifier == 'viewer.currentImage' AND label == 'page-01.png'")
        ).firstMatch
        XCTAssertTrue(image.waitForExistence(timeout: 15))
        XCTAssertTrue(app.otherElements["viewer.touchSurface"].isHittable)
        XCTAssertFalse(app.descendants(matching: .any)["viewer.error"].exists)
    }

    func testMangaSpreadDefaultHasNoBlackBindingGap() throws {
        XCUIDevice.shared.orientation = .portrait
        let app = XCUIApplication()
        app.launchEnvironment["WML2VIEWER_UI_TEST_NO_RESTORE"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_FIXTURE_FOLDER"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_MANGA"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_FORCE_SPREAD"] = "1"
        app.launchEnvironment["WML2VIEWER_UI_TEST_MANGA_PAGE_SPACING"] = "0"
        app.launch()

        XCTAssertTrue(app.descendants(matching: .any)["viewer.currentImage"].waitForExistence(timeout: 15))
        XCUIDevice.shared.orientation = .landscapeLeft
        XCTAssertTrue(
            app.descendants(matching: .any)["uiTest.mangaSpreadReady"]
                .waitForExistence(timeout: 15)
        )
        let surface = app.otherElements["viewer.touchSurface"]
        XCTAssertTrue(surface.waitForExistence(timeout: 5))
        let screenshot = XCUIScreen.main.screenshot().image
        let leftOfBinding = try pixelRGBA(
            in: screenshot,
            at: CGPoint(x: surface.frame.midX - 3, y: surface.frame.midY)
        )
        let rightOfBinding = try pixelRGBA(
            in: screenshot,
            at: CGPoint(x: surface.frame.midX + 3, y: surface.frame.midY)
        )
        XCTAssertGreaterThan(Int(leftOfBinding[0]) + Int(leftOfBinding[1]) + Int(leftOfBinding[2]), 100)
        XCTAssertGreaterThan(Int(rightOfBinding[0]) + Int(rightOfBinding[1]) + Int(rightOfBinding[2]), 100)
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
        let spacing = app.steppers["settings.mangaPageSpacing"]
        XCTAssertTrue(spacing.waitForExistence(timeout: 5))
        XCTAssertTrue(spacing.label.contains("見開き間隔"))
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

    private func pickerFailureDescription(_ app: XCUIApplication) -> String {
        let screenshot = XCTAttachment(screenshot: XCUIScreen.main.screenshot())
        screenshot.name = "UIDocumentPicker failure"
        screenshot.lifetime = .keepAlways
        add(screenshot)
        return "UIDocumentPicker hierarchy:\n\(app.debugDescription)"
    }

    private func pixelRGBA(in image: UIImage, at point: CGPoint) throws -> [UInt8] {
        let cgImage = try XCTUnwrap(image.cgImage)
        let scaleX = CGFloat(cgImage.width) / image.size.width
        let scaleY = CGFloat(cgImage.height) / image.size.height
        let pixelX = min(max(Int(point.x * scaleX), 0), cgImage.width - 1)
        let pixelY = min(max(Int(point.y * scaleY), 0), cgImage.height - 1)
        let crop = try XCTUnwrap(cgImage.cropping(to: CGRect(x: pixelX, y: pixelY, width: 1, height: 1)))
        var rgba = [UInt8](repeating: 0, count: 4)
        let context = try XCTUnwrap(CGContext(
            data: &rgba,
            width: 1,
            height: 1,
            bitsPerComponent: 8,
            bytesPerRow: 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo.byteOrder32Big.rawValue |
                CGImageAlphaInfo.premultipliedLast.rawValue
        ))
        context.draw(crop, in: CGRect(x: 0, y: 0, width: 1, height: 1))
        return rgba
    }
}
