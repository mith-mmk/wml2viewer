import Foundation
import UIKit
import UniformTypeIdentifiers

final class IOSImportCoordinator: NSObject, UIDocumentPickerDelegate {
    static let shared = IOSImportCoordinator()

    private let fileManager = FileManager.default
    private var importing = false
    private var pickerPresented = false
    private var requestPoller: Timer?
    private var pendingImports: [(URL, String)] = []

    private var appSupport: URL {
        fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
    }

    private var documents: URL {
        fileManager.urls(for: .documentDirectory, in: .userDomainMask)[0]
    }

    func start() {
        try? fileManager.createDirectory(at: appSupport, withIntermediateDirectories: true)
        requestPoller?.invalidate()
        requestPoller = Timer.scheduledTimer(withTimeInterval: 0.25, repeats: true) { [weak self] _ in
            self?.pollRequests()
        }
    }

    func requestFolderPicker() {
        enqueuePicker(contentTypes: [.folder])
    }

    func requestFilePicker() {
        enqueuePicker(contentTypes: [.item])
    }

    func receiveExternalURL(_ url: URL) {
        guard url.isFileURL else { return }
        pendingImports.append((url, url.hasDirectoryPath ? "folder" : "file"))
        processNextImport()
    }

    private func enqueuePicker(contentTypes: [UTType]) {
        guard !pickerPresented else { return }
        pickerPresented = true
        presentPickerWhenReady(contentTypes: contentTypes, attemptsRemaining: 20)
    }

    private func presentPicker(contentTypes: [UTType], presenter: UIViewController) {
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: contentTypes, asCopy: false)
        picker.delegate = self
        picker.allowsMultipleSelection = false
        writeImportStatus("presenting")
        presenter.present(picker, animated: true)
    }

    private func presentPickerWhenReady(contentTypes: [UTType], attemptsRemaining: Int) {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            guard let presenter = self.topViewController() else {
                guard attemptsRemaining > 0 else {
                    self.pickerPresented = false
                    self.writeImportStatus("failed")
                    return
                }
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {
                    self.presentPickerWhenReady(
                        contentTypes: contentTypes,
                        attemptsRemaining: attemptsRemaining - 1
                    )
                }
                return
            }
            if presenter.presentedViewController != nil {
                guard attemptsRemaining > 0 else {
                    self.pickerPresented = false
                    self.writeImportStatus("failed")
                    return
                }
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.25) {
                    self.presentPickerWhenReady(
                        contentTypes: contentTypes,
                        attemptsRemaining: attemptsRemaining - 1
                    )
                }
                return
            }
            self.presentPicker(contentTypes: contentTypes, presenter: presenter)
        }
    }

    private func pollRequests() {
        guard !pickerPresented, !importing else { return }
        let folderRequest = appSupport.appendingPathComponent("picker.request")
        let fileRequest = appSupport.appendingPathComponent("filepicker.request")
        if fileManager.fileExists(atPath: folderRequest.path) {
            try? fileManager.removeItem(at: folderRequest)
            enqueuePicker(contentTypes: [.folder])
        } else if fileManager.fileExists(atPath: fileRequest.path) {
            try? fileManager.removeItem(at: fileRequest)
            enqueuePicker(contentTypes: [.item])
        }
    }

    func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
        _ = controller
        pickerPresented = false
        guard let url = urls.first else {
            writeImportStatus("cancelled")
            return
        }
        receiveExternalURL(url)
    }

    func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
        _ = controller
        pickerPresented = false
        writeImportStatus("cancelled")
    }

    private func processNextImport() {
        guard !importing, !pendingImports.isEmpty else { return }
        let (source, kind) = pendingImports.removeFirst()
        importing = true
        writeImportStatus("importing")
        let securityScopeStarted = source.startAccessingSecurityScopedResource()
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            defer {
                if securityScopeStarted {
                    source.stopAccessingSecurityScopedResource()
                }
                DispatchQueue.main.async {
                    self?.importing = false
                    self?.processNextImport()
                }
            }
            do {
                try self?.copySnapshot(source: source, kind: kind)
            } catch {
                self?.writeImportStatus("failed")
                NSLog("wml2viewer iOS import failed")
            }
        }
    }

    private func copySnapshot(source: URL, kind: String) throws {
        let snapshots = documents.appendingPathComponent("snapshots", isDirectory: true)
        let generation = String(UInt64(Date().timeIntervalSince1970 * 1_000_000))
        let staging = snapshots.appendingPathComponent(".staging-\(generation)", isDirectory: true)
        let destination = snapshots.appendingPathComponent(generation, isDirectory: true)
        let importedRoot = staging.appendingPathComponent("folder", isDirectory: true)
        let sourceName = source.lastPathComponent.isEmpty ? "読み込み済みファイル" : source.lastPathComponent

        try fileManager.createDirectory(at: snapshots, withIntermediateDirectories: true)
        try? fileManager.removeItem(at: staging)
        var referenceCommitted = false
        do {
            try fileManager.createDirectory(at: importedRoot, withIntermediateDirectories: true)
            var coordinationError: NSError?
            var copyError: Error?
            let coordinator = NSFileCoordinator(filePresenter: nil)
            coordinator.coordinate(readingItemAt: source, options: [], error: &coordinationError) { coordinatedURL in
                do {
                    if kind == "folder" {
                        let target = staging.appendingPathComponent("folder", isDirectory: true)
                        try? self.fileManager.removeItem(at: target)
                        try self.fileManager.copyItem(at: coordinatedURL, to: target)
                    } else {
                        let target = importedRoot.appendingPathComponent(sourceName)
                        try self.fileManager.copyItem(at: coordinatedURL, to: target)
                    }
                } catch {
                    copyError = error
                }
            }
            if let coordinationError { throw coordinationError }
            if let copyError { throw copyError }

            try? fileManager.removeItem(at: destination)
            try fileManager.moveItem(at: staging, to: destination)

            var reference: [String: Any] = [
                "generation": generation,
                "relative_root": "folder",
                "display_name": sourceName,
                "kind": kind,
                "created_at": Date().timeIntervalSince1970,
            ]
            if kind == "file" {
                reference["selected_relative_path"] = sourceName
            }
            let referenceData = try JSONSerialization.data(withJSONObject: reference, options: [.sortedKeys])
            let temporaryReference = appSupport.appendingPathComponent("current.json.tmp")
            let currentReference = appSupport.appendingPathComponent("current.json")
            try fileManager.createDirectory(at: appSupport, withIntermediateDirectories: true)
            try referenceData.write(to: temporaryReference, options: [.atomic])
            if fileManager.fileExists(atPath: currentReference.path) {
                _ = try fileManager.replaceItemAt(currentReference, withItemAt: temporaryReference)
            } else {
                try fileManager.moveItem(at: temporaryReference, to: currentReference)
            }
            referenceCommitted = true
            try Data().write(to: appSupport.appendingPathComponent("import.ready"), options: [.atomic])
        } catch {
            try? fileManager.removeItem(at: staging)
            if !referenceCommitted {
                try? fileManager.removeItem(at: destination)
            }
            throw error
        }
    }

    private func writeImportStatus(_ state: String) {
        let statusURL = appSupport.appendingPathComponent("import.status.json")
        let payload: [String: String] = ["state": state]
        guard let data = try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys]) else {
            return
        }
        let temporaryURL = appSupport.appendingPathComponent("import.status.json.tmp")
        do {
            try data.write(to: temporaryURL, options: [.atomic])
            if fileManager.fileExists(atPath: statusURL.path) {
                _ = try fileManager.replaceItemAt(statusURL, withItemAt: temporaryURL)
            } else {
                try fileManager.moveItem(at: temporaryURL, to: statusURL)
            }
        } catch {
            try? fileManager.removeItem(at: temporaryURL)
        }
    }

    private func topViewController() -> UIViewController? {
        let root = UIApplication.shared.connectedScenes
            .compactMap({ $0 as? UIWindowScene })
            .first(where: { $0.activationState == .foregroundActive })?
            .windows
            .first(where: { $0.isKeyWindow })?
            .rootViewController
            ?? UIApplication.shared.windows.first(where: { $0.isKeyWindow })?.rootViewController
        guard let root else {
            return nil
        }
        var controller = root
        while let presented = controller.presentedViewController {
            controller = presented
        }
        return controller
    }
}

@_cdecl("wml2viewer_ios_initialize_bridge")
func wml2viewer_ios_initialize_bridge() {
    IOSImportCoordinator.shared.start()
}

@_cdecl("wml2viewer_ios_receive_external_path")
func wml2viewer_ios_receive_external_path(_ path: UnsafePointer<CChar>?) {
    guard let path else { return }
    IOSImportCoordinator.shared.receiveExternalURL(
        URL(fileURLWithPath: String(cString: path))
    )
}

@_cdecl("wml2viewer_ios_request_folder_picker")
func wml2viewer_ios_request_folder_picker() -> Int32 {
    IOSImportCoordinator.shared.requestFolderPicker()
    return 1
}

@_cdecl("wml2viewer_ios_request_file_picker")
func wml2viewer_ios_request_file_picker() -> Int32 {
    IOSImportCoordinator.shared.requestFilePicker()
    return 1
}
