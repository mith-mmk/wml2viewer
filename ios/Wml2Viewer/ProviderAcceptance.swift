#if DEBUG
import Foundation

enum ProviderAcceptanceKind: String, Codable, CaseIterable {
    case local
    case iCloud = "icloud"
    case thirdParty = "third-party"
    case smb
}

enum ProviderAcceptancePhase: String, Codable, CaseIterable {
    case initialAuthorization = "initial"
    case registeredReopen = "reopen"
}

struct ProviderAcceptanceReport: Codable, Equatable {
    static let schemaVersion = 2

    let schemaVersion: Int
    let token: String
    let provider: ProviderAcceptanceKind
    let phase: ProviderAcceptancePhase
    private(set) var sequence: Int
    private(set) var status: String
    private(set) var pickerRequested = false
    private(set) var pickerCancelled = false
    private(set) var registeredRestoreRequested = false
    private(set) var folderEnumeratedItemCount = 0
    private(set) var folderSupportedItemCount = 0
    private(set) var decodedPageCount = 0
    private(set) var movedForward = false
    private(set) var movedBackward = false
    private(set) var filmstripOpened = false
    private(set) var thumbnailDecoded = false
    private(set) var recoverableErrorObserved = false
    private(set) var inputReadyAfterError = false
    private(set) var recoveredAfterError = false

    init(
        token: String,
        provider: ProviderAcceptanceKind,
        phase: ProviderAcceptancePhase = .initialAuthorization
    ) {
        schemaVersion = Self.schemaVersion
        self.token = token
        self.provider = provider
        self.phase = phase
        sequence = 0
        status = "in-progress"
    }

    mutating func recordPickerRequested() {
        pickerRequested = true
        advance()
    }

    mutating func recordPickerCancelled() {
        pickerCancelled = true
        advance()
    }

    mutating func recordRegisteredRestoreRequested() {
        guard phase == .registeredReopen else { return }
        registeredRestoreRequested = true
        advance()
    }

    mutating func recordFolderSnapshot(enumerated: Int, supported: Int) {
        guard hasSourceRequest else { return }
        folderEnumeratedItemCount = max(0, enumerated)
        folderSupportedItemCount = max(0, supported)
        advance()
    }

    mutating func recordDecodeReady(pageCount: Int) {
        guard hasSourceRequest else { return }
        decodedPageCount = max(decodedPageCount, max(0, pageCount))
        if phase == .registeredReopen {
            folderEnumeratedItemCount = max(folderEnumeratedItemCount, max(0, pageCount))
            folderSupportedItemCount = max(folderSupportedItemCount, max(0, pageCount))
        }
        if recoverableErrorObserved { recoveredAfterError = true }
        advance()
    }

    mutating func recordNavigation(from: Int, to: Int) {
        guard hasSourceRequest else { return }
        if to > from { movedForward = true }
        if to < from { movedBackward = true }
        advance()
    }

    mutating func recordFilmstripOpened() {
        guard hasSourceRequest else { return }
        filmstripOpened = true
        advance()
    }

    mutating func recordThumbnailDecoded() {
        guard hasSourceRequest else { return }
        thumbnailDecoded = true
        advance()
    }

    mutating func recordRecoverableError(inputReady: Bool) {
        guard hasSourceRequest else { return }
        recoverableErrorObserved = true
        inputReadyAfterError = inputReadyAfterError || inputReady
        advance()
    }

    private mutating func advance() {
        sequence &+= 1
        let openedThroughExpectedRoute = switch phase {
        case .initialAuthorization:
            pickerRequested
        case .registeredReopen:
            registeredRestoreRequested && !pickerRequested
        }
        status = openedThroughExpectedRoute &&
            folderSupportedItemCount >= 2 &&
            decodedPageCount >= 1 &&
            movedForward && movedBackward &&
            filmstripOpened && thumbnailDecoded
            ? "passed" : "in-progress"
    }

    private var hasSourceRequest: Bool {
        switch phase {
        case .initialAuthorization: pickerRequested
        case .registeredReopen: registeredRestoreRequested
        }
    }
}

@MainActor
final class ProviderAcceptanceRecorder {
    static let resultFileName = "wml2viewer-provider-acceptance.json"

    private var report: ProviderAcceptanceReport
    private let destination: URL
    var phase: ProviderAcceptancePhase { report.phase }

    static func fromProcessArguments(
        _ arguments: [String] = ProcessInfo.processInfo.arguments,
        cachesDirectory: URL? = nil
    ) -> ProviderAcceptanceRecorder? {
        guard let flag = arguments.firstIndex(of: "--provider-acceptance") else { return nil }
        let tokenIndex = arguments.index(after: flag)
        let providerIndex = arguments.index(after: tokenIndex)
        let phaseIndex = arguments.index(after: providerIndex)
        guard arguments.indices.contains(tokenIndex),
              arguments.indices.contains(providerIndex),
              isSafeToken(arguments[tokenIndex]),
              let provider = ProviderAcceptanceKind(rawValue: arguments[providerIndex]) else {
            return nil
        }
        let phase = arguments.indices.contains(phaseIndex)
            ? ProviderAcceptancePhase(rawValue: arguments[phaseIndex])
            : .initialAuthorization
        guard let phase else { return nil }
        let directory: URL
        if let cachesDirectory {
            directory = cachesDirectory
        } else {
            guard let caches = try? FileManager.default.url(
                for: .cachesDirectory,
                in: .userDomainMask,
                appropriateFor: nil,
                create: true
            ) else { return nil }
            directory = caches
        }
        return ProviderAcceptanceRecorder(
            token: arguments[tokenIndex],
            provider: provider,
            phase: phase,
            destination: directory.appendingPathComponent(resultFileName)
        )
    }

    private static func isSafeToken(_ token: String) -> Bool {
        !token.isEmpty && token.count <= 128 && token.unicodeScalars.allSatisfy {
            CharacterSet.alphanumerics.union(CharacterSet(charactersIn: "-_")).contains($0)
        }
    }

    init(
        token: String,
        provider: ProviderAcceptanceKind,
        phase: ProviderAcceptancePhase = .initialAuthorization,
        destination: URL
    ) {
        report = ProviderAcceptanceReport(token: token, provider: provider, phase: phase)
        self.destination = destination
        persist()
    }

    func pickerRequested() {
        report.recordPickerRequested()
        persist()
    }

    func pickerCancelled() {
        report.recordPickerCancelled()
        persist()
    }

    func registeredRestoreRequested() {
        report.recordRegisteredRestoreRequested()
        persist()
    }

    func folderCommitted(_ snapshot: SourceSnapshot) {
        report.recordFolderSnapshot(
            enumerated: snapshot.enumeratedItemCount,
            supported: snapshot.supportedItemCount
        )
        persist()
    }

    func registeredSourceRestored(enumerated: Int, supported: Int) {
        report.recordFolderSnapshot(enumerated: enumerated, supported: supported)
        persist()
    }

    func decodeReady(pageCount: Int) {
        report.recordDecodeReady(pageCount: pageCount)
        persist()
    }

    func navigated(from: Int, to: Int) {
        report.recordNavigation(from: from, to: to)
        persist()
    }

    func filmstripOpened() {
        report.recordFilmstripOpened()
        persist()
    }

    func thumbnailDecoded() {
        report.recordThumbnailDecoded()
        persist()
    }

    func recoverableError(inputReady: Bool) {
        report.recordRecoverableError(inputReady: inputReady)
        persist()
    }

    private func persist() {
        do {
            let data = try JSONEncoder().encode(report)
            try data.write(to: destination, options: .atomic)
        } catch {
            NSLog("WML2VIEWER_PROVIDER_ACCEPTANCE_WRITE_FAILED")
        }
    }
}
#endif
