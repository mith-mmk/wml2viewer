import Foundation
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

struct MobileFileTypePolicy: Sendable {
    private static let archiveFormats = [
        "zip": "zip", "cbz": "zip", "lha": "lha", "lzh": "lzh", "wmltxt": "wmltxt",
    ]
    private static let mimeByExtension = [
        "avif": "image/avif",
        "bmp": "image/bmp", "dib": "image/bmp",
        "dng": "image/dng",
        "gif": "image/gif",
        "heic": "image/heic", "heif": "image/heif",
        "ico": "image/x-icon",
        "jpe": "image/jpeg", "jpeg": "image/jpeg", "jpg": "image/jpeg",
        "png": "image/png",
        "tif": "image/tiff", "tiff": "image/tiff",
        "webp": "image/webp",
    ]
    private static let extensionByMime = [
        "image/avif": "avif",
        "image/bmp": "bmp",
        "image/dng": "dng", "image/x-adobe-dng": "dng",
        "image/gif": "gif",
        "image/heic": "heic", "image/heif": "heif",
        "image/x-icon": "ico", "image/vnd.microsoft.icon": "ico",
        "image/jpeg": "jpg",
        "image/png": "png",
        "image/tiff": "tif",
        "image/webp": "webp",
    ]

    static let shared = MobileFileTypePolicy(
        internalImageExtensions: (try? NativeBridge.internalDecoderExtensions()) ?? [],
        imageIOImageExtensions: ImageIOCodecRouter.supportedImageExtensions
    )

    let internalImageExtensions: Set<String>
    let imageIOImageExtensions: Set<String>

    var imageExtensions: Set<String> {
        internalImageExtensions.union(imageIOImageExtensions)
    }

    init(internalImageExtensions: Set<String>, imageIOImageExtensions: Set<String>) {
        self.internalImageExtensions = Set(internalImageExtensions.map(Self.normalizeExtension))
        self.imageIOImageExtensions = Set(imageIOImageExtensions.map(Self.normalizeExtension))
    }

    func isImage(_ name: String, declaredMime: String? = nil) -> Bool {
        if imageExtensions.contains(Self.fileExtension(name)) { return true }
        guard let mime = Self.normalizeMime(declaredMime),
              let canonicalExtension = Self.extensionByMime[mime] else { return false }
        return imageExtensions.contains(canonicalExtension)
    }

    func archiveFormat(for name: String) -> String? {
        Self.archiveFormats[Self.fileExtension(name)]
    }

    func isArchive(_ name: String) -> Bool {
        archiveFormat(for: name) != nil
    }

    func isSupported(_ name: String, declaredMime: String? = nil) -> Bool {
        isImage(name, declaredMime: declaredMime) || archiveFormat(for: name) != nil
    }

    func mimeType(for name: String, declared: String? = nil) -> String? {
        if let mime = Self.normalizeMime(declared),
           let canonicalExtension = Self.extensionByMime[mime],
           imageExtensions.contains(canonicalExtension) {
            return mime
        }
        return Self.mimeByExtension[Self.fileExtension(name)]
    }

    private static func fileExtension(_ name: String) -> String {
        URL(fileURLWithPath: name).pathExtension.lowercased()
    }

    private static func normalizeExtension(_ value: String) -> String {
        value.trimmingCharacters(in: CharacterSet(charactersIn: ". ")).lowercased()
    }

    private static func normalizeMime(_ value: String?) -> String? {
        value?.split(separator: ";", maxSplits: 1).first?
            .trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    }
}

enum ImageIOCodecRouter {
    private static let mobileCandidateExtensions: Set<String> = ["avif", "dng", "heic", "heif"]

    static let supportedImageExtensions: Set<String> = {
        let identifiers = (CGImageSourceCopyTypeIdentifiers() as? [String]) ?? []
        let availableTypes = identifiers.compactMap(UTType.init)
        return Set(mobileCandidateExtensions.filter { candidateExtension in
            guard let candidateType = UTType(filenameExtension: candidateExtension) else {
                return false
            }
            return availableTypes.contains { availableType in
                candidateType == availableType || candidateType.conforms(to: availableType)
                    || availableType.conforms(to: candidateType)
            }
        })
    }()

    static func decodeOrder(routing: String) -> [CodecBackend] {
        CodecRouting(configValue: routing).decodeOrder
    }

    static func decode(_ data: Data, routing: String = "DEFAULT") throws -> CGImage {
        guard let source = CGImageSourceCreateWithData(data as CFData, nil),
              let image = CGImageSourceCreateImageAtIndex(source, 0, nil) else {
            throw DocumentSourceError.unsupportedItem
        }
        if routing == "OS_ONLY" || routing == "OS_FIRST" || routing == "DEFAULT" { return image }
        return image
    }

    static func supports(_ type: String) -> Bool {
        ((CGImageSourceCopyTypeIdentifiers() as? [String]) ?? []).contains(type)
    }
}
