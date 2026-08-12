import SwiftUI

struct ViewerSurface: View {
    @ObservedObject var store: ViewerStore
    @State private var dragStart: CGSize = .zero
    @State private var gestureScale: CGFloat = 1
    @State private var runtimeFit: DisplayFit?

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                Color.black.ignoresSafeArea()
                if !store.displaySpreadImages.isEmpty {
                    HStack(spacing: 0) {
                        ForEach(Array(store.displaySpreadImages.enumerated()), id: \.offset) { _, image in
                            Image(decorative: image, scale: 1, orientation: .up)
                                .resizable()
                                .aspectRatio(contentMode: runtimeFit == nil ? fitMode : (runtimeFit == .original ? .fit : .fill))
                                .frame(maxWidth: .infinity, maxHeight: .infinity)
                        }
                    }
                        .scaleEffect(store.zoom * gestureScale)
                        .offset(store.pan)
                        .accessibilityLabel(store.pages.indices.contains(store.currentIndex) ? store.pages[store.currentIndex].displayName : String(localized: "Image"))
                } else if store.isLoading {
                    ProgressView().tint(.white)
                } else {
                    ContentUnavailableView(String(localized: "No document"), systemImage: "photo", description: Text(String(localized: "Open a file or folder from Files")))
                }

            }
            .contentShape(Rectangle())
            // SwiftUI fallback keeps the bottom-center action reachable when UIKit's
            // recognizer is hosted inside an empty-state view hierarchy.
            .simultaneousGesture(
                SpatialTapGesture().onEnded { value in
                    guard store.config.touchZonesEnabled,
                          geometry.size.height > 0,
                          value.location.y >= geometry.size.height * 2.0 / 3.0,
                          value.location.x >= geometry.size.width / 3.0,
                          value.location.x < geometry.size.width * 2.0 / 3.0 else { return }
                    store.showFilmstrip = true
                }
            )
            .overlay {
                TouchGestureBridge(
                    pinchEnabled: store.config.pinchZoomEnabled,
                    panEnabled: store.config.panEnabled,
                    swipeEnabled: store.config.swipeEnabled,
                    canPan: { store.zoom > 1.01 },
                    canSwipe: { store.zoom <= 1.01 },
                    onPinch: { scale in guard store.interactionReady else { return }; store.zoom = min(max(store.zoom * scale, 1), 8) },
                    onPan: { translation, ended in
                        if store.zoom > 1.01 { store.pan.width += translation.width; store.pan.height += translation.height }
                        if ended { dragStart = store.pan }
                    },
                    onSwipe: { direction in direction == .left ? store.next() : store.previous() },
                    onLongPress: { if store.config.longPressQuickMenuEnabled { store.showQuickMenu = true } },
                    onDoubleTap: {
                        runtimeFit = (runtimeFit ?? store.config.fit) == .original ? .contain : .original
                        store.zoom = 1
                        store.pan = .zero
                    },
                    onZoneTap: { row, col in
                        guard store.config.touchZonesEnabled, store.interactionReady else { return }
                        switch TouchZoneResolver.defaultAction(row: row, column: col) {
                        case .previous: store.previous()
                        case .next: store.next()
                        case .openFiler: store.requestFilePicker()
                        case .settings: store.showSettings = true
                        case .filmstrip: store.showFilmstrip = true
                        case nil: break
                        }
                    },
                    onGenerationChanged: { dragStart = .zero }
                )
                .accessibilityIdentifier("viewer.touchSurface")
                .allowsHitTesting(!store.showSettings && !store.showFilmstrip && !store.isPickerPresented)
            }
            .overlay(alignment: .top) {
                if store.config.showTopChrome {
                    topChrome
                }
            }
            .overlay(alignment: .bottom) { bottomChrome }
            .onAppear { store.updateViewport(geometry.size) }
            .onChange(of: geometry.size) { _, size in store.updateViewport(size) }
        }
        .onChange(of: store.currentIndex) { _, _ in runtimeFit = nil }
        .toolbar(.hidden, for: .navigationBar)
    }

    private var fitMode: ContentMode {
        switch runtimeFit ?? store.config.fit {
        case .contain, .original: .fit
        case .width, .height: .fill
        }
    }

    private var topChrome: some View {
        VStack {
            HStack {
                Button { store.requestFilePicker() } label: {
                    Label(String(localized: "Open"), systemImage: "folder")
                }
                .frame(minHeight: 44)
                .accessibilityIdentifier("viewer.open")
                .accessibilityHint(String(localized: "Open a document from Files"))
                Spacer()
                Button { store.showSettings = true } label: {
                    Image(systemName: "gearshape")
                }
                .frame(minWidth: 44, minHeight: 44)
                .accessibilityIdentifier("viewer.settings")
                .accessibilityLabel(String(localized: "Settings"))
            }
            .padding(.horizontal)
            .padding(.top, 8)
            .foregroundStyle(.white)
            Spacer()
        }
    }

    private var bottomChrome: some View {
        HStack(spacing: 24) {
            Button { store.previous() } label: { Image(systemName: "chevron.left") }
                .accessibilityIdentifier("viewer.previous")
            Button { store.showFilmstrip = true } label: { Image(systemName: "rectangle.stack") }
                .accessibilityIdentifier("viewer.filmstrip")
            Button { store.requestFolderPicker() } label: { Image(systemName: "folder.badge.plus") }
                .accessibilityIdentifier("viewer.folder")
            Button { store.next() } label: { Image(systemName: "chevron.right") }
                .accessibilityIdentifier("viewer.next")
        }
        .font(.title2)
        .buttonStyle(.borderedProminent)
        .tint(.black.opacity(0.65))
        .foregroundStyle(.white)
        .padding(.bottom, 18)
    }

}

struct FilmstripView: View {
    @ObservedObject var store: ViewerStore

    var body: some View {
        NavigationStack {
            List(Array(store.pages.enumerated()), id: \.element.id) { index, item in
                Button {
                    store.select(index: index)
                } label: {
                    HStack {
                        Text("\(index + 1)").monospacedDigit().foregroundStyle(.secondary)
                        Text(item.displayName).lineLimit(1)
                        Spacer()
                        if index == store.currentIndex { Image(systemName: "checkmark") }
                    }
                }
                .accessibilityAddTraits(index == store.currentIndex ? .isSelected : [])
            }
            .accessibilityIdentifier("filmstrip.list")
            .navigationTitle(String(localized: "Pages"))
            .toolbar {
                ToolbarItem(placement: .principal) {
                    Text(String(localized: "Pages", locale: store.config.locale))
                        .accessibilityIdentifier("filmstrip.title")
                }
            }
        }
        .accessibilityIdentifier("filmstrip.panel")
    }
}
