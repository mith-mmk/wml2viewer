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
                    await store.installUITestFixtureIfRequested()
                    store.startProviderAcceptanceIfNeeded()
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
        GeometryReader { geometry in
            let pinsFilmstrip = ViewerResponsiveLayout.pinsFilmstrip(
                isPad: UIDevice.current.userInterfaceIdiom == .pad,
                width: geometry.size.width,
                enabled: store.config.showFilmstrip && store.showFilmstrip
            )
            Group {
                if pinsFilmstrip {
                    HStack(spacing: 0) {
                        ViewerSurface(store: store, filmstripIsPinned: true)
                        Divider()
                        FilmstripView(store: store)
                            .frame(width: min(360, max(280, geometry.size.width * 0.28)))
                    }
                } else {
                    ViewerSurface(store: store)
                }
            }
            .disabled(store.isPickerPresented)
            .sheet(isPresented: filmstripPresentation(isPinned: pinsFilmstrip)) {
                FilmstripView(store: store)
                    .presentationDetents([.medium, .large])
            }
        }
        .sheet(isPresented: $store.showSettings) {
            SettingsView(store: store)
        }
        .confirmationDialog(String(localized: "Quick menu"), isPresented: $store.showQuickMenu, titleVisibility: .visible) {
            Button(String(localized: "Open")) { store.requestFilePicker() }
            Button(String(localized: "Choose folder")) { store.requestFolderPicker() }
                .accessibilityIdentifier("quickMenu.folder")
            if store.hasRestorableLocation {
                Button(String(localized: "Restore last location")) {
                    Task { await store.restoreLastLocation() }
                }
                .accessibilityIdentifier("quickMenu.restoreLastLocation")
            }
            Button(String(localized: "Manage Files")) { store.requestFileManagement() }
                .accessibilityIdentifier("quickMenu.manageFiles")
            Button(String(localized: "Pages")) { store.openFilmstrip() }
                .accessibilityIdentifier("quickMenu.pages")
            Button(String(localized: store.animationEnabled ? "Pause animation" : "Play animation")) {
                store.toggleAnimation()
            }
            Button(String(localized: store.grayscaleEnabled ? "Color image" : "Grayscale image")) {
                store.toggleGrayscale()
            }
            Menu(String(localized: "Export")) {
                ForEach(ImageIOCodecRouter.availableExportFormats) { format in
                    Button(format.localizedLabel) {
                        store.prepareExport(format: format)
                    }
                }
            }
            Button(String(localized: "Cancel"), role: .cancel) {}
        }
        .sheet(item: $store.exportItem, onDismiss: { store.finishExport() }) { item in
            SystemShareSheet(activityItems: [item.url])
        }
        .fullScreenCover(item: $store.pendingPicker) { presentation in
            Group {
                if presentation.request == .manageFiles {
                    ZStack(alignment: .topTrailing) {
                        DocumentBrowserView { result in
                            store.finishPicker(presentation, result)
                        } onUnexpectedDismissal: {
                            store.pickerDidDismiss(presentation.id)
                        }
                        Button {
                            store.finishPicker(presentation, nil)
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .font(.title2)
                                .symbolRenderingMode(.hierarchical)
                        }
                        .buttonStyle(.bordered)
                        .accessibilityLabel(String(localized: "Close"))
                        .accessibilityIdentifier("documentBrowser.close")
                        .padding()
                    }
                } else {
                    ZStack(alignment: .bottom) {
                        SystemDocumentPicker(
                            presentation: presentation
                        ) { result in
                            store.finishPicker(presentation, result)
                        } onUnexpectedDismissal: {
                            store.pickerDidDismiss(presentation.id)
                        }
                        if let guidance = PickerFolderGuidance.message(
                            for: presentation,
                            locale: store.config.locale
                        ) {
                            Label(guidance, systemImage: "folder.badge.questionmark")
                                .font(.callout)
                                .multilineTextAlignment(.leading)
                                .padding(.horizontal, 14)
                                .padding(.vertical, 10)
                                .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 14))
                                .shadow(radius: 4, y: 2)
                                .padding(.horizontal, 16)
                                .padding(.bottom, 92)
                                .allowsHitTesting(false)
                                .accessibilityElement(children: .combine)
                                // Keep the explanatory text available as the
                                // element's label on iOS versions that expose
                                // Label's icon and text as separate nodes.
                                .accessibilityLabel(guidance)
                                .accessibilityIdentifier("documentPicker.folderGuidance")
                        }
                    }
                }
            }
            // Capture the presentation token in the content itself. A global
            // `onDismiss` can otherwise mistake a delayed callback from the
            // first picker for the automatically queued containing-folder
            // picker and close the new presentation.
            .onDisappear {
                store.pickerDidDismiss(presentation.id)
            }
        }
        .overlay(alignment: .bottom) {
            if let error = store.errorMessage {
                HStack(spacing: 12) {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .font(.callout)
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(error)
                        .accessibilityAddTraits(.isStaticText)
                        .accessibilityIdentifier("viewer.error")
                    Button(String(localized: "Retry")) { store.retryCurrentSource() }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("viewer.error.retry")
                    if !store.filesPickerRecoveryRequired {
                        Button(String(localized: "Open Files")) { store.requestFilePicker() }
                            .buttonStyle(.borderedProminent)
                            .accessibilityIdentifier("viewer.error.openFiles")
                    }
                    if !store.filesPickerRecoveryRequired {
                        Button(String(localized: "Choose folder")) { store.requestFolderPicker() }
                            .buttonStyle(.bordered)
                            .accessibilityIdentifier("viewer.error.chooseFolder")
                    }
                    if store.hasRestorableLocation {
                        Button(String(localized: "Restore last location")) {
                            Task { await store.restoreLastLocation() }
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("viewer.error.restoreLastLocation")
                    }
                    if store.filesPickerRecoveryRequired {
                        Button(String(localized: "Reset Files recovery")) {
                            store.resetFilesRecovery()
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("viewer.error.resetFilesRecovery")
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(.ultraThinMaterial, in: Capsule())
                .padding(.bottom, 24)
            } else if let notice = store.sourceNoticeMessage {
                HStack(spacing: 10) {
                    Label(notice, systemImage: "folder.badge.questionmark")
                        .font(.callout)
                        .accessibilityAddTraits(.isStaticText)
                        .accessibilityIdentifier("viewer.sourceNotice")
                    if !store.filesPickerRecoveryRequired {
                        Button(String(localized: "Open Files")) {
                            store.requestFilePicker()
                        }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("viewer.sourceNotice.openFiles")
                    }
                    if store.hasRestorableLocation {
                        Button(String(localized: "Restore last location")) {
                            Task { await store.restoreLastLocation() }
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("viewer.sourceNotice.restoreLastLocation")
                    }
                    if store.filesPickerRecoveryRequired {
                        Button(String(localized: "Reset Files recovery")) {
                            store.resetFilesRecovery()
                        }
                        .buttonStyle(.bordered)
                        .accessibilityIdentifier("viewer.sourceNotice.resetFilesRecovery")
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
                .background(.ultraThinMaterial, in: Capsule())
                .padding(.bottom, 24)
            }
        }
        .overlay {
            if let progress = store.sourceOpeningProgress {
                ZStack {
                    Color.black.opacity(0.42)
                        .ignoresSafeArea()
                    VStack(spacing: 14) {
                        ProgressView()
                            .controlSize(.large)
                        Text(progress.title)
                            .font(.headline)
                        Text(progress.detail)
                            .font(.callout)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.center)
                        Button(String(localized: "Cancel"), role: .cancel) {
                            store.cancelSourceOpening()
                        }
                        .buttonStyle(.borderedProminent)
                        .accessibilityIdentifier("sourceOpening.cancel")
                    }
                    .padding(24)
                    .frame(maxWidth: 420)
                    .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 18))
                    .padding()
                    .accessibilityElement(children: .contain)
                    .accessibilityIdentifier("sourceOpening.progress")
                }
                .transition(.opacity)
            }
        }
        #if DEBUG
        .overlay(alignment: .topLeading) {
            if store.uiTestPickerFixtureReady {
                Text("ready")
                    .frame(width: 1, height: 1)
                    .opacity(0.01)
                    .accessibilityIdentifier("uiTest.pickerFixtureReady")
            }
            if store.uiTestMangaSpreadReady {
                Text("spread-ready")
                    .frame(width: 1, height: 1)
                    .opacity(0.01)
                    .accessibilityIdentifier("uiTest.mangaSpreadReady")
            }
        }
        #endif
        .environment(\.locale, store.config.locale)
        .preferredColorScheme(store.config.theme.colorScheme)
        .onChange(of: scenePhase) { _, phase in
            store.handleScenePhase(phase)
            if phase == .active { Task { await store.reconcileExternalChanges() } }
        }
    }

    private func filmstripPresentation(isPinned: Bool) -> Binding<Bool> {
        Binding(
            get: { store.showFilmstrip && !isPinned },
            set: { presented in
                if !presented { store.showFilmstrip = false }
            }
        )
    }
}

private struct SystemShareSheet: UIViewControllerRepresentable {
    let activityItems: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: activityItems, applicationActivities: nil)
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}
