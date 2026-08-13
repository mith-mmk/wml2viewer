import SwiftUI
import UniformTypeIdentifiers
import UIKit

/// Native Files management browser. The primary open action uses the mixed
/// file/folder document picker; copy/move/rename/delete/share remain owned by
/// UIDocumentBrowserViewController and its File Provider.
struct DocumentBrowserView: UIViewControllerRepresentable {
    var onPick: (Result<URL, Error>?) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(onPick: onPick) }
    func makeUIViewController(context: Context) -> UIDocumentBrowserViewController {
        // Management may still open a selected document, but it never implies
        // access to that document's containing directory.
        let controller = UIDocumentBrowserViewController(forOpening: [UTType.item])
        controller.delegate = context.coordinator
        controller.allowsDocumentCreation = false
        controller.allowsPickingMultipleItems = false
        return controller
    }
    func updateUIViewController(_ controller: UIDocumentBrowserViewController, context: Context) {}

    final class Coordinator: NSObject, UIDocumentBrowserViewControllerDelegate {
        let onPick: (Result<URL, Error>?) -> Void
        private let completionGate = PickerCompletionGate()
        init(onPick: @escaping (Result<URL, Error>?) -> Void) { self.onPick = onPick }
        func documentBrowser(_ controller: UIDocumentBrowserViewController, didPickDocumentsAt documentURLs: [URL]) {
            completionGate.perform { onPick(documentURLs.first.map { .success($0) }) }
        }
        func documentBrowser(_ controller: UIDocumentBrowserViewController, didRequestDocumentCreationWithHandler importHandler: @escaping (URL?, UIDocumentBrowserViewController.ImportMode) -> Void) {
            importHandler(nil, UIDocumentBrowserViewController.ImportMode.none)
        }
        func documentBrowser(_ controller: UIDocumentBrowserViewController, didImportDocumentAt documentURL: URL, toDestinationURL destinationURL: URL) {}
        func documentBrowser(_ controller: UIDocumentBrowserViewController, failedToImportDocumentAt documentURL: URL, error: Error?) {
            completionGate.perform { onPick(.failure(error ?? CocoaError(.fileReadUnknown))) }
        }
    }
}
