import SwiftUI
import UIKit

@main
struct Wml2ViewerApp: App {
    @StateObject private var store = ViewerStore()

    init() {
        #if DEBUG
        NativeBridgeSelfTest.runIfRequested()
        #endif
    }

    var body: some Scene {
        WindowGroup {
            ContentView(store: store)
                .task {
                    await store.restoreLastSource()
                    #if DEBUG
                    store.applyUITestOverrides()
                    #endif
                }
                .onOpenURL { store.openExternalURL($0) }
        }
    }
}

#if DEBUG
private enum NativeBridgeSelfTest {
    private struct Result: Codable {
        let token: String
        let status: String
        let message: String?
    }

    static func runIfRequested() {
        let arguments = ProcessInfo.processInfo.arguments
        guard let flagIndex = arguments.firstIndex(of: "--native-self-test") else { return }
        let tokenIndex = arguments.index(after: flagIndex)
        let token = arguments.indices.contains(tokenIndex) ? arguments[tokenIndex] : "missing-token"

        do {
            let session = try NativeSession()
            let request = try session.nextRequest()
            session.cancel(request)
            session.close()
            write(Result(token: token, status: "ok", message: nil))
            NSLog("WML2VIEWER_NATIVE_SELF_TEST_OK")
        } catch {
            write(Result(token: token, status: "failed", message: error.localizedDescription))
            NSLog("WML2VIEWER_NATIVE_SELF_TEST_FAILED: %@", error.localizedDescription)
        }
    }

    private static func write(_ result: Result) {
        do {
            let cache = try FileManager.default.url(
                for: .cachesDirectory,
                in: .userDomainMask,
                appropriateFor: nil,
                create: true
            )
            let destination = cache.appendingPathComponent("wml2viewer-device-smoke.json")
            let data = try JSONEncoder().encode(result)
            try data.write(to: destination, options: .atomic)
        } catch {
            NSLog("WML2VIEWER_NATIVE_SELF_TEST_RESULT_WRITE_FAILED: %@", error.localizedDescription)
        }
    }
}
#endif

struct ContentView: View {
    @ObservedObject var store: ViewerStore
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        ViewerSurface(store: store)
            .disabled(store.isPickerPresented)
        .sheet(isPresented: $store.showSettings) {
            SettingsView(store: store)
        }
        .sheet(isPresented: $store.showFilmstrip) {
            FilmstripView(store: store)
                .presentationDetents([.medium, .large])
        }
        .confirmationDialog(String(localized: "Quick menu"), isPresented: $store.showQuickMenu, titleVisibility: .visible) {
            Button(String(localized: "Open")) { store.requestFilePicker() }
            Button(String(localized: "Pages")) { store.showFilmstrip = true }
                .accessibilityIdentifier("quickMenu.pages")
            Button(String(localized: store.animationEnabled ? "Pause animation" : "Play animation")) {
                store.toggleAnimation()
            }
            Button(String(localized: store.grayscaleEnabled ? "Color image" : "Grayscale image")) {
                store.toggleGrayscale()
            }
            Button(String(localized: "Export")) { store.prepareExport() }
                .disabled(store.image == nil)
            Button(String(localized: "Cancel"), role: .cancel) {}
        }
        .sheet(item: $store.exportItem, onDismiss: { store.finishExport() }) { item in
            SystemShareSheet(activityItems: [item.url])
        }
        .sheet(item: $store.pendingPicker) { request in
            Group {
                if request == .file {
                    DocumentBrowserView { result in store.finishPicker(result) }
                } else {
                    SystemDocumentPicker(request: request) { result in
                        store.finishPicker(result)
                    }
                }
            }
        }
        .overlay(alignment: .bottom) {
            if let error = store.errorMessage {
                Label(error, systemImage: "exclamationmark.triangle")
                    .font(.callout)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                    .background(.ultraThinMaterial, in: Capsule())
                    .padding(.bottom, 24)
                    .accessibilityAddTraits(.isStaticText)
            }
        }
        .environment(\.locale, store.config.locale)
        .preferredColorScheme(store.config.theme.colorScheme)
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { Task { await store.reconcileExternalChanges() } }
        }
    }
}

private struct SystemShareSheet: UIViewControllerRepresentable {
    let activityItems: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: activityItems, applicationActivities: nil)
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}
