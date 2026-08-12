import Foundation
import CoreGraphics
import ImageIO

enum ImageIOCodecRouter {
    static func decode(_ data: Data, routing: String = "DEFAULT") throws -> CGImage {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            throw DocumentSourceError.unsupportedItem
        }
        if routing == "OS_ONLY" || routing == "OS_FIRST" || routing == "DEFAULT" { return image }
        return image
    }

    static func supports(_ type: String) -> Bool {
        ["public.jpeg", "public.png", "com.compuserve.gif", "org.webmproject.webp", "public.heic"].contains(type)
    }
}
