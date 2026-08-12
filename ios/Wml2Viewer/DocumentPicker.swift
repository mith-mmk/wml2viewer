import SwiftUI
import UniformTypeIdentifiers
import UIKit

struct SystemDocumentPicker: UIViewControllerRepresentable {
    let request: PickerRequest
    let completion: (Result<URL, Error>?) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(completion: completion) }

    func makeUIViewController(context: Context) -> UIDocumentPickerViewController {
        let types: [UTType] = request == .folder ? [.folder] : [.item]
        let controller = UIDocumentPickerViewController(forOpeningContentTypes: types, asCopy: false)
        controller.delegate = context.coordinator
        controller.allowsMultipleSelection = false
        return controller
    }

    func updateUIViewController(_ controller: UIDocumentPickerViewController, context: Context) {}

    final class Coordinator: NSObject, UIDocumentPickerDelegate {
        let completion: (Result<URL, Error>?) -> Void

        init(completion: @escaping (Result<URL, Error>?) -> Void) { self.completion = completion }

        func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
            guard let url = urls.first else { completion(nil); return }
            completion(.success(url))
        }

        func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) { completion(nil) }
    }
}
