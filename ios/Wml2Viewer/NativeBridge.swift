import Foundation

/// Swift ownership wrappers for the checked iOS C ABI. Handles never escape these types.
final class NativeSession {
    private(set) var handle: UInt64

    init() throws {
        let value = wml2viewer_ios_session_create()
        guard value != 0 else { throw NativeBridgeError.creationFailed }
        handle = value
    }

    func nextRequest() throws -> UInt64 {
        let request = wml2viewer_ios_request_next(handle)
        guard request != 0, wml2viewer_ios_request_begin(handle, request) != 0 else {
            throw NativeBridgeError.requestFailed
        }
        return request
    }

    func cancel(_ request: UInt64) { _ = wml2viewer_ios_request_cancel(handle, request) }

    func close() {
        guard handle != 0 else { return }
        _ = wml2viewer_ios_session_release(handle)
        handle = 0
    }

    deinit { close() }
}

final class NativeImage {
    private(set) var handle: UInt64
    init(handle: UInt64) { self.handle = handle }

    var width: Int { Int(wml2viewer_ios_image_width(handle)) }
    var height: Int { Int(wml2viewer_ios_image_height(handle)) }
    var stride: Int { Int(wml2viewer_ios_image_stride(handle)) }
    var frameCount: Int { Int(wml2viewer_ios_image_frame_count(handle)) }
    var loopCount: Int64 { wml2viewer_ios_image_loop_count(handle) }

    func frameDurationMilliseconds(at index: Int) throws -> UInt64 {
        var duration: UInt64 = 0
        guard wml2viewer_ios_image_frame_duration_ms(handle, index, &duration) != 0 else {
            throw NativeBridgeError.invalidBuffer
        }
        return duration
    }

    func frame(at index: Int) throws -> NativeImage {
        let child = wml2viewer_ios_image_frame(handle, index)
        guard child != 0 else { throw NativeBridgeError.invalidBuffer }
        return NativeImage(handle: child)
    }

    func copyRGBA() throws -> Data {
        var pointer: UnsafePointer<UInt8>?
        var length = 0
        guard wml2viewer_ios_image_rgba(handle, &pointer, &length) != 0,
              let pointer else { throw NativeBridgeError.invalidBuffer }
        return Data(bytes: pointer, count: length)
    }

    func close() {
        guard handle != 0 else { return }
        _ = wml2viewer_ios_image_release(handle)
        handle = 0
    }

    deinit { close() }
}

final class NativeBytes {
    private(set) var handle: UInt64
    init(handle: UInt64) { self.handle = handle }

    func copy() throws -> Data {
        var pointer: UnsafePointer<UInt8>?
        var length = 0
        guard wml2viewer_ios_bytes_view(handle, &pointer, &length) != 0,
              let pointer else { throw NativeBridgeError.invalidBuffer }
        return Data(bytes: pointer, count: length)
    }

    func close() {
        guard handle != 0 else { return }
        _ = wml2viewer_ios_bytes_release(handle)
        handle = 0
    }

    deinit { close() }
}

final class NativeArchive {
    private(set) var handle: UInt64
    init(handle: UInt64) { self.handle = handle }

    var entryCount: Int { Int(wml2viewer_ios_archive_entry_count(handle)) }

    func entryName(at index: Int) throws -> String {
        var required = 0
        guard wml2viewer_ios_archive_entry_name(handle, index, nil, 0, &required) != 0,
              required <= 64 * 1024 else { throw NativeBridgeError.invalidBuffer }
        var bytes = [UInt8](repeating: 0, count: required)
        guard wml2viewer_ios_archive_entry_name(handle, index, &bytes, bytes.count, &required) != 0,
              let value = String(bytes: bytes.prefix(required), encoding: .utf8) else {
            throw NativeBridgeError.invalidBuffer
        }
        return value
    }

    func decode(session: NativeSession, request: UInt64, index: Int, mime: String?) throws -> NativeImage {
        let imageHandle: UInt64
        if let mime {
            let mimeBytes = Array(mime.utf8)
            imageHandle = mimeBytes.withUnsafeBufferPointer { buffer in
                wml2viewer_ios_archive_entry_decode(session.handle, request, handle, index,
                                                     buffer.baseAddress, buffer.count)
            }
        } else {
            imageHandle = wml2viewer_ios_archive_entry_decode(session.handle, request, handle, index, nil, 0)
        }
        guard imageHandle != 0 else { throw NativeBridgeError.decodeFailed }
        return NativeImage(handle: imageHandle)
    }

    func materialize(session: NativeSession, request: UInt64, index: Int) throws -> NativeBytes {
        let bytesHandle = wml2viewer_ios_archive_entry_materialize(
            session.handle, request, handle, index
        )
        guard bytesHandle != 0 else { throw NativeBridgeError.decodeFailed }
        return NativeBytes(handle: bytesHandle)
    }

    func close() {
        guard handle != 0 else { return }
        _ = wml2viewer_ios_archive_release(handle)
        handle = 0
    }

    deinit { close() }
}

enum NativeBridgeError: LocalizedError {
    case creationFailed, requestFailed, decodeFailed, invalidBuffer
    var errorDescription: String? {
        switch self {
        case .creationFailed: "Native session could not be created"
        case .requestFailed: "Native request could not be started"
        case .decodeFailed: "Native decoder rejected the item"
        case .invalidBuffer: "Native decoder returned an invalid buffer"
        }
    }
}

enum NativeReadingLayout: Int32 { case auto = 0, single = 1, spread = 2 }
enum NativeReadingDirection: Int32 { case leftToRight = 0, rightToLeft = 1 }

struct NativeReadingPage {
    let sourceID: Int64
    let portrait: Bool
    let cover: Bool
}

struct NativeReadingPlan: Equatable {
    let anchorIndex: Int
    let logicalIndices: [Int]
    let visualIndices: [Int]
    let previousAnchorIndex: Int?
    let nextAnchorIndex: Int?
    let preloadIndices: [Int]
}

enum NativeReadingPlanner {
    private static let headerCount = 8

    static func plan(
        pages: [NativeReadingPage], currentIndex: Int, landscape: Bool,
        layout: NativeReadingLayout = .auto,
        direction: NativeReadingDirection = .rightToLeft,
        coverAlone: Bool = true, maximumPrefetchSpreads: Int = 1
    ) -> NativeReadingPlan? {
        guard !pages.isEmpty, pages.count <= 4_096, pages.indices.contains(currentIndex),
              (0...64).contains(maximumPrefetchSpreads) else { return nil }
        let ids = pages.map(\.sourceID)
        let portraits = pages.map { UInt8($0.portrait ? 1 : 0) }
        let covers = pages.map { UInt8($0.cover ? 1 : 0) }
        var required = 0
        let queried = ids.withUnsafeBufferPointer { idBuffer in
            portraits.withUnsafeBufferPointer { portraitBuffer in
                covers.withUnsafeBufferPointer { coverBuffer in
                    wml2viewer_ios_plan_reading_v1(
                        idBuffer.baseAddress, portraitBuffer.baseAddress, coverBuffer.baseAddress,
                        pages.count, Int32(currentIndex), landscape ? 1 : 0, layout.rawValue,
                        direction.rawValue, coverAlone ? 1 : 0, Int32(maximumPrefetchSpreads),
                        nil, 0, &required
                    )
                }
            }
        }
        guard queried != 0, required >= headerCount, required <= 8 + pages.count * 3 else { return nil }
        var wire = [Int32](repeating: 0, count: required)
        let copied = ids.withUnsafeBufferPointer { idBuffer in
            portraits.withUnsafeBufferPointer { portraitBuffer in
                covers.withUnsafeBufferPointer { coverBuffer in
                    wire.withUnsafeMutableBufferPointer { output in
                        wml2viewer_ios_plan_reading_v1(
                            idBuffer.baseAddress, portraitBuffer.baseAddress, coverBuffer.baseAddress,
                            pages.count, Int32(currentIndex), landscape ? 1 : 0, layout.rawValue,
                            direction.rawValue, coverAlone ? 1 : 0, Int32(maximumPrefetchSpreads),
                            output.baseAddress, output.count, &required
                        )
                    }
                }
            }
        }
        guard copied != 0 else { return nil }
        return decode(wire, pageCount: pages.count, currentIndex: currentIndex, maximumPrefetchSpreads: maximumPrefetchSpreads)
    }

    static func decode(_ wire: [Int32], pageCount: Int, currentIndex: Int, maximumPrefetchSpreads: Int) -> NativeReadingPlan? {
        guard wire.count >= headerCount, wire[0] == 1, Int(wire[1]) == wire.count else { return nil }
        let logicalCount = Int(wire[5]), visualCount = Int(wire[6]), preloadCount = Int(wire[7])
        guard (1...2).contains(logicalCount), visualCount == logicalCount,
              preloadCount >= 0, preloadCount <= maximumPrefetchSpreads * 2,
              headerCount + logicalCount + visualCount + preloadCount == wire.count else { return nil }
        var cursor = headerCount
        func values(_ count: Int) -> [Int] {
            defer { cursor += count }
            return wire[cursor..<(cursor + count)].map(Int.init)
        }
        let logical = values(logicalCount), visual = values(visualCount), preload = values(preloadCount)
        guard (logical + visual + preload).allSatisfy({ pagesRange(pageCount).contains($0) }),
              logical.first == Int(wire[2]), logical.contains(currentIndex),
              Set(logical).count == logical.count, Set(visual) == Set(logical) else { return nil }
        func optionalIndex(_ raw: Int32) -> Int? {
            raw == -1 ? nil : (pagesRange(pageCount).contains(Int(raw)) ? Int(raw) : nil)
        }
        let previous = optionalIndex(wire[3]), next = optionalIndex(wire[4])
        if (wire[3] != -1 && previous == nil) || (wire[4] != -1 && next == nil) { return nil }
        return NativeReadingPlan(anchorIndex: Int(wire[2]), logicalIndices: logical, visualIndices: visual,
                                 previousAnchorIndex: previous, nextAnchorIndex: next, preloadIndices: preload)
    }

    private static func pagesRange(_ count: Int) -> Range<Int> { 0..<count }
}

enum NativeBridge {
    static func decode(path: URL, mime: String? = nil, session: NativeSession, request: UInt64) throws -> NativeImage {
        let pathBytes = Array(path.path.utf8)
        let imageHandle: UInt64 = pathBytes.withUnsafeBufferPointer { pathBuffer in
            if let mime {
                let mimeBytes = Array(mime.utf8)
                return mimeBytes.withUnsafeBufferPointer { mimeBuffer in
                    wml2viewer_ios_decode_local(session.handle, request,
                                                pathBuffer.baseAddress, pathBuffer.count,
                                                mimeBuffer.baseAddress, mimeBuffer.count)
                }
            }
            return wml2viewer_ios_decode_local(session.handle, request,
                                               pathBuffer.baseAddress, pathBuffer.count, nil, 0)
        }
        guard imageHandle != 0 else { throw NativeBridgeError.decodeFailed }
        return NativeImage(handle: imageHandle)
    }

    static func openArchive(path: URL, format: String, session: NativeSession, request: UInt64) throws -> NativeArchive {
        let pathBytes = Array(path.path.utf8)
        let formatBytes = Array(format.utf8)
        let archiveHandle = pathBytes.withUnsafeBufferPointer { pathBuffer in
            formatBytes.withUnsafeBufferPointer { formatBuffer in
                wml2viewer_ios_archive_open_local(session.handle, request, pathBuffer.baseAddress, pathBuffer.count,
                                                   formatBuffer.baseAddress, formatBuffer.count)
            }
        }
        guard archiveHandle != 0 else { throw NativeBridgeError.decodeFailed }
        return NativeArchive(handle: archiveHandle)
    }
}
