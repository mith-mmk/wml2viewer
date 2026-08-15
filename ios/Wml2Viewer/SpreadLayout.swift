import CoreGraphics

enum MangaPageSpacing {
    static let defaultPoints: Double = 0
    static let minimumPoints: Double = 0
    static let maximumPoints: Double = 64

    static func clamp(_ value: Double) -> Double {
        guard value.isFinite else { return defaultPoints }
        return min(max(value, minimumPoints), maximumPoints)
    }
}

enum SpreadLayout {
    static func pageRects(
        imageSizes: [CGSize],
        surfaceSize: CGSize,
        fit: DisplayFit,
        spacing: CGFloat
    ) -> [CGRect] {
        guard !imageSizes.isEmpty,
              surfaceSize.width > 0,
              surfaceSize.height > 0,
              imageSizes.allSatisfy({ $0.width > 0 && $0.height > 0 }) else {
            return []
        }

        let safeSpacing = max(0, spacing)
        let totalSpacing = safeSpacing * CGFloat(max(0, imageSizes.count - 1))
        let nativeWidth = imageSizes.reduce(0) { $0 + $1.width }
        let nativeHeight = imageSizes.map(\.height).max() ?? 0
        let availableWidth = max(0, surfaceSize.width - totalSpacing)
        guard nativeWidth > 0, nativeHeight > 0, availableWidth > 0 else { return [] }

        let scale: CGFloat
        switch fit {
        case .contain:
            scale = min(availableWidth / nativeWidth, surfaceSize.height / nativeHeight)
        case .width:
            scale = availableWidth / nativeWidth
        case .height:
            scale = surfaceSize.height / nativeHeight
        case .original:
            scale = 1
        }

        let contentWidth = nativeWidth * scale + totalSpacing
        let contentHeight = nativeHeight * scale
        var x = (surfaceSize.width - contentWidth) / 2
        let groupTop = (surfaceSize.height - contentHeight) / 2
        return imageSizes.map { imageSize in
            let pageSize = CGSize(width: imageSize.width * scale, height: imageSize.height * scale)
            let rect = CGRect(
                x: x,
                y: groupTop + (contentHeight - pageSize.height) / 2,
                width: pageSize.width,
                height: pageSize.height
            )
            x += pageSize.width + safeSpacing
            return rect
        }
    }
}
