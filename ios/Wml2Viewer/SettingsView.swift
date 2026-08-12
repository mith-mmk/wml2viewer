import SwiftUI

struct SettingsView: View {
    @ObservedObject var store: ViewerStore
    @Environment(\.dismiss) private var dismiss

    private var displayTitle: String {
        if store.config.language == "ja" { return "表示" }
        return String(localized: "Display", locale: store.config.locale)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section(header: Text(displayTitle).accessibilityIdentifier("settings.display")) {
                    Picker(String(localized: "Fit", locale: store.config.locale), selection: Binding(get: { store.config.fit }, set: { var c = store.config; c.fit = $0; store.update(c) })) {
                        ForEach(DisplayFit.allCases, id: \.self) { Text($0.rawValue.capitalized).tag($0) }
                    }
                    Toggle(String(localized: "Show top controls", locale: store.config.locale), isOn: Binding(get: { store.config.showTopChrome }, set: { var c = store.config; c.showTopChrome = $0; store.update(c) }))
                    Toggle(String(localized: "Keep screen awake", locale: store.config.locale), isOn: Binding(get: { store.config.keepScreenOn }, set: { var c = store.config; c.keepScreenOn = $0; store.update(c) }))
                }
                Section(String(localized: "Manga", locale: store.config.locale)) {
                    Toggle(String(localized: "Manga mode", locale: store.config.locale), isOn: Binding(get: { store.config.mangaEnabled }, set: { var c = store.config; c.mangaEnabled = $0; store.update(c) }))
                    Toggle(String(localized: "Right to left", locale: store.config.locale), isOn: Binding(get: { store.config.mangaRTL }, set: { var c = store.config; c.mangaRTL = $0; store.update(c) }))
                }
                Section(String(localized: "Touch", locale: store.config.locale)) {
                    Toggle(String(localized: "3×3 touch zones", locale: store.config.locale), isOn: Binding(get: { store.config.touchZonesEnabled }, set: { var c = store.config; c.touchZonesEnabled = $0; store.update(c) }))
                    Toggle(String(localized: "Swipe navigation", locale: store.config.locale), isOn: Binding(get: { store.config.swipeEnabled }, set: { var c = store.config; c.swipeEnabled = $0; store.update(c) }))
                    Toggle(String(localized: "Pinch zoom", locale: store.config.locale), isOn: Binding(get: { store.config.pinchZoomEnabled }, set: { var c = store.config; c.pinchZoomEnabled = $0; store.update(c) }))
                    Toggle(String(localized: "Pan while zoomed", locale: store.config.locale), isOn: Binding(get: { store.config.panEnabled }, set: { var c = store.config; c.panEnabled = $0; store.update(c) }))
                    Toggle(String(localized: "Long press quick menu", locale: store.config.locale), isOn: Binding(get: { store.config.longPressQuickMenuEnabled }, set: { var c = store.config; c.longPressQuickMenuEnabled = $0; store.update(c) }))
                    Stepper("\(String(localized: "Prefetch spreads", locale: store.config.locale)): \(store.config.prefetchSpreads)", value: Binding(get: { store.config.prefetchSpreads }, set: { var c = store.config; c.prefetchSpreads = $0; store.update(c) }), in: 0...8)
                }
                Section(String(localized: "Files and restoration", locale: store.config.locale)) {
                    Toggle(String(localized: "Remember last location", locale: store.config.locale), isOn: Binding(get: { store.config.rememberLastLocation }, set: { var c = store.config; c.rememberLastLocation = $0; store.update(c) }))
                    Button(String(localized: "Choose folder", locale: store.config.locale)) {
                        store.showSettings = false
                        store.requestFolderPicker()
                    }
                }
                Section(String(localized: "Appearance", locale: store.config.locale)) {
                    Picker(String(localized: "Theme", locale: store.config.locale), selection: Binding(get: { store.config.theme }, set: { var c = store.config; c.theme = $0; store.update(c) })) {
                        ForEach(ThemeMode.allCases, id: \.self) { Text($0.rawValue).tag($0) }
                    }
                }
                Section(String(localized: "Codec", locale: store.config.locale)) {
                    Picker(String(localized: "Routing", locale: store.config.locale), selection: Binding(get: { store.config.codecRouting }, set: { var c = store.config; c.codecRouting = $0; store.update(c) })) {
                        ForEach(["DEFAULT", "INTERNAL_FIRST", "OS_FIRST", "INTERNAL_ONLY", "OS_ONLY"], id: \.self) { Text($0).tag($0) }
                    }
                }
                Section(String(localized: "Language and appearance", locale: store.config.locale)) {
                    Picker(String(localized: "Language", locale: store.config.locale), selection: Binding(get: { store.config.language }, set: { var c = store.config; c.language = $0; store.update(c) })) {
                        Text(String(localized: "System", locale: store.config.locale)).tag("system")
                        Text("English").tag("en")
                        Text("日本語").tag("ja")
                    }
                }
                Section(String(localized: "Cache", locale: store.config.locale)) {
                    Stepper("\(String(localized: "Cache limit", locale: store.config.locale)): \(Int((store.config.cacheLimitBytes ?? 134_217_728) / 1_048_576)) MiB", value: Binding(get: { Int((store.config.cacheLimitBytes ?? 134_217_728) / 1_048_576) }, set: { var c = store.config; c.cacheLimitBytes = UInt64(max(64, min($0, 2048))) * 1_048_576; store.update(c) }), in: 64...2048, step: 64)
                }
                Section(String(localized: "About", locale: store.config.locale)) {
                    LabeledContent("wml2viewer", value: "0.0.19")
                }
            }
            .navigationTitle(String(localized: "Settings", locale: store.config.locale))
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(String(localized: "Done", locale: store.config.locale)) { dismiss(); store.showSettings = false }
                        .accessibilityIdentifier("settings.done")
                }
            }
        }
        .accessibilityIdentifier("settings.panel")
        .accessibilityValue(String(localized: "Display", locale: store.config.locale))
        .accessibilityIdentifier("settings.panel")
    }
}
