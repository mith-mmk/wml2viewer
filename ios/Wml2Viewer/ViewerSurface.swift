import SwiftUI

struct ViewerSurface: View {
    @ObservedObject var store: ViewerStore
    var filmstripIsPinned = false
    @State private var dragStart: CGSize = .zero
    @State private var gestureScale: CGFloat = 1
    @State private var runtimeFit: DisplayFit?

    var body: some View {
        GeometryReader { geometry in
            ZStack {
                Color.black.ignoresSafeArea()
                if !store.displaySpreadImages.isEmpty {
                    Canvas { context, size in
                        let images = store.displaySpreadImages
                        let rects = SpreadLayout.pageRects(
                            imageSizes: images.map {
                                CGSize(width: $0.width, height: $0.height)
                            },
                            surfaceSize: size,
                            fit: runtimeFit ?? store.config.fit,
                            spacing: images.count > 1
                                ? CGFloat(store.config.mangaPageSpacing) : 0
                        )
                        for (image, rect) in zip(images, rects) {
                            context.draw(
                                Image(decorative: image, scale: 1, orientation: .up),
                                in: rect
                            )
                        }
                    }
                    .scaleEffect(store.zoom * gestureScale)
                    .offset(store.pan)
                    .accessibilityElement(children: .ignore)
                    .accessibilityLabel(store.pages.indices.contains(store.currentIndex) ? store.pages[store.currentIndex].displayName : String(localized: "Image"))
                    .accessibilityIdentifier("viewer.currentImage")
                } else if store.isLoading {
                    ProgressView().tint(.white)
                } else {
                    ContentUnavailableView(String(localized: "No document"), systemImage: "photo", description: Text(String(localized: "Open a file or folder from Files")))
                }

            }
            .contentShape(Rectangle())
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
                        case .filmstrip: store.openFilmstrip()
                        case nil: break
                        }
                    },
                    onGenerationChanged: { dragStart = .zero }
                )
                .accessibilityIdentifier("viewer.touchSurface")
                .accessibilityLabel(String(localized: "Viewer"))
                .accessibilityValue(store.pagePositionAccessibilityValue)
                .allowsHitTesting(!store.showSettings && (!store.showFilmstrip || filmstripIsPinned) && !store.isPickerPresented)
            }
            .onAppear { store.updateViewport(geometry.size) }
            .onChange(of: geometry.size) { _, size in store.updateViewport(size) }
        }
        .onChange(of: store.currentIndex) { _, _ in runtimeFit = nil }
        .toolbar(.hidden, for: .navigationBar)
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
                        Group {
                            if let thumbnail = store.thumbnail(for: item) {
                                Image(decorative: thumbnail, scale: 1)
                                    .resizable()
                                    .scaledToFit()
                            } else {
                                Image(systemName: item.isArchive ? "archivebox" : "photo")
                                    .foregroundStyle(.secondary)
                            }
                        }
                        .frame(width: 48, height: 56)
                        .accessibilityElement(children: .ignore)
                        .accessibilityLabel(String(localized: "Thumbnail"))
                        .accessibilityIdentifier("filmstrip.thumbnail.\(index)")
                        Text("\(index + 1)").monospacedDigit().foregroundStyle(.secondary)
                        Text(item.displayName).lineLimit(1)
                        Spacer()
                        if index == store.currentIndex { Image(systemName: "checkmark") }
                    }
                }
                .accessibilityAddTraits(index == store.currentIndex ? .isSelected : [])
                .onAppear { store.requestThumbnail(for: item) }
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
