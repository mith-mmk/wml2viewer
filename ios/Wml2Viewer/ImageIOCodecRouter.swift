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

    /// A listed file is a manifest whose entries live beside it. It is not a
    /// self-contained archive and therefore needs an explicit folder grant.
    func isListedFile(_ name: String) -> Bool {
        Self.fileExtension(name) == "wmltxt"
    }

    func isSelfContainedArchive(_ name: String) -> Bool {
        archiveFormat(for: name) != nil && !isListedFile(name)
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
    enum ExportFormat: String, CaseIterable, Identifiable {
        case png
        case jpeg
        case webpLossy
        case webpLossless

        var id: String { rawValue }

        var fileExtension: String {
            switch self {
            case .png: "png"
            case .jpeg: "jpg"
            case .webpLossy, .webpLossless: "webp"
            }
        }

        var type: UTType {
            switch self {
            case .png: .png
            case .jpeg: .jpeg
            case .webpLossy, .webpLossless: UTType(filenameExtension: "webp") ?? .data
            }
        }

        var localizedLabel: String {
            switch self {
            case .png: String(localized: "Export PNG")
            case .jpeg: String(localized: "Export JPEG")
            case .webpLossy: String(localized: "Export WebP (lossy)")
            case .webpLossless: String(localized: "Export WebP (lossless)")
            }
        }
    }

    static let mobileProbeCandidates: Set<String> = [
        "avif", "bmp", "dng", "gif", "heic", "heif", "ico", "jpeg", "jpg", "png", "webp",
    ]

    static let supportedImageExtensions: Set<String> = {
        capabilityProbe()
    }()

    static let availableExportFormats: [ExportFormat] = {
        guard let image = probeImage() else { return [] }
        return ExportFormat.allCases.filter { encodedFixture(image: image, type: $0.type) != nil }
    }()

    /// Probe ImageIO against an actual generated image instead of trusting
    /// the static UTI table alone. Decode-only formats remain eligible when
    /// ImageIO advertises them but has no encoder.
    static func capabilityProbe() -> Set<String> {
        let identifiers = (CGImageSourceCopyTypeIdentifiers() as? [String]) ?? []
        let availableTypes = identifiers.compactMap(UTType.init)
        let destinationIdentifiers =
            (CGImageDestinationCopyTypeIdentifiers() as? [String]) ?? []
        let destinationTypes = destinationIdentifiers.compactMap(UTType.init)
        return Set(mobileProbeCandidates.filter { candidateExtension in
            guard let candidateType = UTType(filenameExtension: candidateExtension),
                  availableTypes.contains(where: {
                      candidateType == $0 || candidateType.conforms(to: $0)
                          || $0.conforms(to: candidateType)
                  }) else { return false }
            let hasEncoder = destinationTypes.contains(where: {
                candidateType == $0 || candidateType.conforms(to: $0)
                    || $0.conforms(to: candidateType)
            })
            guard hasEncoder, let image = probeImage() else {
                // A decoder-only type is still useful when ImageIO advertises it.
                return candidateExtension == "avif" || candidateExtension == "dng"
            }
            if let data = encodedFixture(image: image, type: candidateType) {
                return CGImageSourceCreateWithData(data as CFData, nil) != nil
            }
            return false
        })
    }

    private static func probeImage() -> CGImage? {
        guard let context = CGContext(
            data: nil, width: 16, height: 16, bitsPerComponent: 8, bytesPerRow: 64,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else { return nil }
        context.setFillColor(CGColor(red: 0.1, green: 0.6, blue: 0.9, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: 16, height: 16))
        return context.makeImage()
    }

    private static func encodedFixture(image: CGImage, type: UTType) -> Data? {
        encode(image, type: type)
    }

    static func encode(_ image: CGImage, format: ExportFormat) throws -> Data {
        guard let data = encode(image, type: format.type) else {
            throw DocumentSourceError.unsupportedItem
        }
        return data
    }

    private static func encode(_ image: CGImage, type: UTType) -> Data? {
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(
            data, type.identifier as CFString, 1, nil
        ) else { return nil }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else { return nil }
        return data as Data
    }

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

    /// Detect an animated container even when ImageIO exposes only a poster
    /// frame. This lets OS_FIRST fall back to the internal decoder while
    /// OS_ONLY reports a clear, actionable error.
    static func encodedAnimationHint(_ data: Data) -> Bool {
        let bytes = [UInt8](data)
        if bytes.starts(with: [0x47, 0x49, 0x46, 0x38]) { // GIF8
            return bytes.dropFirst().filter { $0 == 0x2c }.count > 1
        }
        if bytes.starts(with: [0x89, 0x50, 0x4e, 0x47]) { // PNG
            return data.range(of: Data("acTL".utf8)) != nil
        }
        if bytes.count >= 12,
           bytes[0..<4].elementsEqual([0x52, 0x49, 0x46, 0x46]),
           bytes[8..<12].elementsEqual([0x57, 0x45, 0x42, 0x50]) { // RIFF/WEBP
            return data.range(of: Data("ANMF".utf8)) != nil
        }
        return false
    }
}
