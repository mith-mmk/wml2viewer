import SwiftUI
import UniformTypeIdentifiers
import UIKit

struct SystemDocumentPicker: UIViewControllerRepresentable {
    let presentation: PickerPresentation
    let completion: (Result<URL, Error>?) -> Void
    let onUnexpectedDismissal: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(completion: completion, onUnexpectedDismissal: onUnexpectedDismissal)
    }

    static func dismantleUIViewController(_ controller: UIDocumentPickerViewController, coordinator: Coordinator) {
        coordinator.notifyUnexpectedDismissalIfNeeded()
    }

    func makeUIViewController(context: Context) -> UIDocumentPickerViewController {
        let types: [UTType] = switch presentation.request {
        case .openTarget: [.item]
        case .containingFolder: [.folder]
        case .manageFiles: [.item]
        }
        let controller = UIDocumentPickerViewController(forOpeningContentTypes: types, asCopy: false)
        controller.delegate = context.coordinator
        controller.allowsMultipleSelection = false
        if let initialDirectoryURL = presentation.initialDirectoryURL {
            controller.directoryURL = initialDirectoryURL
        }
        return controller
    }

    func updateUIViewController(_ controller: UIDocumentPickerViewController, context: Context) {}

    final class Coordinator: NSObject, UIDocumentPickerDelegate {
        let completion: (Result<URL, Error>?) -> Void
        let onUnexpectedDismissal: () -> Void
        private let completionGate = PickerCompletionGate()

        init(
            completion: @escaping (Result<URL, Error>?) -> Void,
            onUnexpectedDismissal: @escaping () -> Void
        ) {
            self.completion = completion
            self.onUnexpectedDismissal = onUnexpectedDismissal
        }

        func notifyUnexpectedDismissalIfNeeded() {
            guard !completionGate.isCompleted else { return }
            completionGate.perform { onUnexpectedDismissal() }
        }

        func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
            completionGate.perform { completion(urls.first.map { .success($0) }) }
        }

        func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
            completionGate.perform { completion(nil) }
        }
    }
}
