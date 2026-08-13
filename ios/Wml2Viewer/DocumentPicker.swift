import SwiftUI
import UniformTypeIdentifiers
import UIKit

struct SystemDocumentPicker: UIViewControllerRepresentable {
    let presentation: PickerPresentation
    let completion: (Result<URL, Error>?) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(completion: completion) }

    func makeUIViewController(context: Context) -> UIDocumentPickerViewController {
        let types: [UTType] = switch presentation.request {
        case .openTarget: [.folder, .item]
        case .containingFolder: [.folder]
        case .manageFiles: [.item]
        }
        let controller = UIDocumentPickerViewController(forOpeningContentTypes: types, asCopy: false)
        controller.delegate = context.coordinator
        controller.allowsMultipleSelection = false
        if presentation.request == .containingFolder {
            controller.directoryURL = presentation.initialDirectoryURL
        }
        return controller
    }

    func updateUIViewController(_ controller: UIDocumentPickerViewController, context: Context) {}

    final class Coordinator: NSObject, UIDocumentPickerDelegate {
        let completion: (Result<URL, Error>?) -> Void
        private let completionGate = PickerCompletionGate()

        init(completion: @escaping (Result<URL, Error>?) -> Void) { self.completion = completion }

        func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
            completionGate.perform { completion(urls.first.map { .success($0) }) }
        }

        func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
            completionGate.perform { completion(nil) }
        }
    }
}
