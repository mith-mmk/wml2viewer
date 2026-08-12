import Foundation
import SwiftUI

struct EntryRef: Hashable, Codable, Sendable {
    let sourceID: UUID
    let opaqueEntryID: String
}

enum PickerRequest: Identifiable, Hashable {
    case file
    case folder

    var id: String {
        switch self {
        case .file: "file"
        case .folder: "folder"
        }
    }
}

enum DisplayFit: String, Codable, CaseIterable {
    case contain, width, height, original
}

/// Runtime double-tap override. It never mutates the persisted initial fit.
enum FitOverridePolicy {
    static func next(current: DisplayFit) -> DisplayFit {
        current == .original ? .contain : .original
    }
}

enum ThemeMode: String, Codable, CaseIterable {
    case cinematicDark, light, system

    var colorScheme: ColorScheme? {
        switch self {
        case .cinematicDark: .dark
        case .light: .light
        case .system: nil
        }
    }
}

struct MobileConfigV1: Codable, Equatable {
    var schemaVersion = 1
    var fit: DisplayFit = .contain
    var showTopChrome = true
    var showFilmstrip = true
    var keepScreenOn = false
    var mangaEnabled = false
    var mangaRTL = true
    var coverAlone = true
    var prefetchSpreads = 1
    var theme: ThemeMode = .cinematicDark
    var language = "system"
    var rememberLastLocation = true
    var cacheLimitBytes: UInt64? = nil
    var codecRouting = "DEFAULT"
    var touchZonesEnabled = true
    var swipeEnabled = false
    var pinchZoomEnabled = true
    var panEnabled = true
    var longPressQuickMenuEnabled = true

    var locale: Locale {
        language == "system" ? .autoupdatingCurrent : Locale(identifier: language)
    }

    private enum CodingKeys: String, CodingKey {
        case schemaVersion, fit, showTopChrome, showFilmstrip, keepScreenOn, mangaEnabled, mangaRTL,
             coverAlone, prefetchSpreads, theme, language, rememberLastLocation, cacheLimitBytes,
             codecRouting, touchZonesEnabled, swipeEnabled, pinchZoomEnabled, panEnabled,
             longPressQuickMenuEnabled
    }

    init() {}

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        schemaVersion = try c.decodeIfPresent(Int.self, forKey: .schemaVersion) ?? 1
        fit = try c.decodeIfPresent(DisplayFit.self, forKey: .fit) ?? .contain
        showTopChrome = try c.decodeIfPresent(Bool.self, forKey: .showTopChrome) ?? true
        showFilmstrip = try c.decodeIfPresent(Bool.self, forKey: .showFilmstrip) ?? true
        keepScreenOn = try c.decodeIfPresent(Bool.self, forKey: .keepScreenOn) ?? false
        mangaEnabled = try c.decodeIfPresent(Bool.self, forKey: .mangaEnabled) ?? false
        mangaRTL = try c.decodeIfPresent(Bool.self, forKey: .mangaRTL) ?? true
        coverAlone = try c.decodeIfPresent(Bool.self, forKey: .coverAlone) ?? true
        prefetchSpreads = try c.decodeIfPresent(Int.self, forKey: .prefetchSpreads) ?? 1
        theme = try c.decodeIfPresent(ThemeMode.self, forKey: .theme) ?? .cinematicDark
        language = try c.decodeIfPresent(String.self, forKey: .language) ?? "system"
        rememberLastLocation = try c.decodeIfPresent(Bool.self, forKey: .rememberLastLocation) ?? true
        cacheLimitBytes = try c.decodeIfPresent(UInt64.self, forKey: .cacheLimitBytes)
        codecRouting = try c.decodeIfPresent(String.self, forKey: .codecRouting) ?? "DEFAULT"
        touchZonesEnabled = try c.decodeIfPresent(Bool.self, forKey: .touchZonesEnabled) ?? true
        swipeEnabled = try c.decodeIfPresent(Bool.self, forKey: .swipeEnabled) ?? false
        pinchZoomEnabled = try c.decodeIfPresent(Bool.self, forKey: .pinchZoomEnabled) ?? true
        panEnabled = try c.decodeIfPresent(Bool.self, forKey: .panEnabled) ?? true
        longPressQuickMenuEnabled = try c.decodeIfPresent(Bool.self, forKey: .longPressQuickMenuEnabled) ?? true
    }
}

enum ViewerAction: Equatable {
    case previous
    case next
    case openFiler
    case settings
    case filmstrip
}

struct TouchZone: Equatable {
    let row: Int
    let column: Int
}

enum TouchZoneResolver {
    static func zone(at point: CGPoint, in size: CGSize) -> TouchZone? {
        guard size.width > 0, size.height > 0,
              point.x >= 0, point.y >= 0,
              point.x < size.width, point.y < size.height else { return nil }
        return TouchZone(
            row: min(2, Int(point.y / (size.height / 3))),
            column: min(2, Int(point.x / (size.width / 3)))
        )
    }

    /// Physical left/right placement deliberately does not mirror in RTL locales.
    static func defaultAction(row: Int, column: Int) -> ViewerAction? {
        guard (0..<3).contains(row), (0..<3).contains(column) else { return nil }
        if column == 0 { return .previous }
        if column == 2 { return .next }
        switch row {
        case 0: return .openFiler
        case 1: return .settings
        case 2: return .filmstrip
        default: return nil
        }
    }
}

struct BookmarkRecord: Codable, Equatable, Sendable {
    let sourceID: UUID
    let bookmark: Data
    let displayName: String
    let isFolder: Bool
    var opaqueEntryID: String?
    var logicalPageIndex: Int
}

struct PageItem: Identifiable, Hashable, Sendable {
    let id: String
    let url: URL
    let displayName: String
    let isArchive: Bool

    var isSupported: Bool {
        let ext = url.pathExtension.lowercased()
        return ["jpg", "jpeg", "png", "gif", "webp", "bmp", "tif", "tiff", "avif", "heif", "heic", "zip", "lha", "lzh", "wmltxt"].contains(ext)
    }
}
