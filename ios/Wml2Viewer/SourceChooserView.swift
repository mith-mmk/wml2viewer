import SwiftUI

struct SourceChooserView: View {
    @ObservedObject var store: ViewerStore
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                Section(String(localized: "Registered locations", locale: store.config.locale)) {
                    if store.registeredSources.isEmpty {
                        ContentUnavailableView(
                            String(localized: "No registered locations", locale: store.config.locale),
                            systemImage: "folder.badge.plus",
                            description: Text(
                                String(
                                    localized: "Folders and archives are registered after they open successfully.",
                                    locale: store.config.locale
                                )
                            )
                        )
                        .accessibilityIdentifier("sourceChooser.empty")
                    } else {
                        ForEach(store.registeredSources) { source in
                            registeredSourceRow(source)
                                .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                                    Button(role: .destructive) {
                                        store.removeRegisteredSource(source.id)
                                    } label: {
                                        Label(
                                            String(localized: "Remove registration", locale: store.config.locale),
                                            systemImage: "trash"
                                        )
                                    }
                                }
                        }
                    }
                }

                Section(String(localized: "Add from Files", locale: store.config.locale)) {
                    Button {
                        store.addFolderFromSourceChooser()
                    } label: {
                        Label(String(localized: "Add folder", locale: store.config.locale), systemImage: "folder.badge.plus")
                    }
                    .disabled(store.filesPickerRecoveryRequired)
                    .accessibilityIdentifier("sourceChooser.addFolder")

                    Button {
                        store.openFileFromSourceChooser()
                    } label: {
                        Label(String(localized: "Open file", locale: store.config.locale), systemImage: "doc")
                    }
                    .disabled(store.filesPickerRecoveryRequired)
                    .accessibilityIdentifier("sourceChooser.openFile")

                    Button {
                        store.manageFilesFromSourceChooser()
                    } label: {
                        Label(String(localized: "Manage Files", locale: store.config.locale), systemImage: "folder")
                    }
                    .disabled(store.filesPickerRecoveryRequired)
                    .accessibilityIdentifier("sourceChooser.manageFiles")
                }

                if store.filesPickerRecoveryRequired {
                    Section(String(localized: "Files recovery", locale: store.config.locale)) {
                        Text(
                            String(
                                localized: "Files browsing is unavailable. Registered locations can still be opened directly.",
                                locale: store.config.locale
                            )
                        )
                        .foregroundStyle(.secondary)
                        Button(String(localized: "Reset Files recovery", locale: store.config.locale)) {
                            store.resetFilesRecovery()
                        }
                        .accessibilityIdentifier("sourceChooser.resetFilesRecovery")
                    }
                }
            }
            .navigationTitle(String(localized: "Registered locations", locale: store.config.locale))
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(String(localized: "Done", locale: store.config.locale)) {
                        dismiss()
                    }
                    .accessibilityIdentifier("sourceChooser.done")
                }
            }
        }
        .accessibilityIdentifier("sourceChooser.panel")
    }

    @ViewBuilder
    private func registeredSourceRow(_ source: RegisteredSourceSummary) -> some View {
        HStack(spacing: 4) {
            Button {
                store.openRegisteredSource(source.id)
            } label: {
                HStack(spacing: 12) {
                    Image(systemName: source.kind == .folder ? "folder" : "doc.zipper")
                        .font(.title3)
                        .frame(width: 28)
                    VStack(alignment: .leading, spacing: 3) {
                        Text(source.displayName)
                            .lineLimit(2)
                        Label(
                            source.status.localizedLabel(locale: store.config.locale),
                            systemImage: source.status.systemImage
                        )
                        .font(.caption)
                        .foregroundStyle(source.status.tint)
                    }
                    Spacer(minLength: 8)
                    Image(systemName: "chevron.right")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.tertiary)
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityIdentifier("sourceChooser.source.\(source.id.uuidString)")

            Menu {
                Button {
                    store.retryRegisteredSource(source.id)
                } label: {
                    Label(String(localized: "Retry", locale: store.config.locale), systemImage: "arrow.clockwise")
                }
                Button {
                    store.reauthorizeRegisteredSource(source.id)
                } label: {
                    Label(String(localized: "Reauthorize", locale: store.config.locale), systemImage: "key")
                }
                Button(role: .destructive) {
                    store.removeRegisteredSource(source.id)
                } label: {
                    Label(String(localized: "Remove registration", locale: store.config.locale), systemImage: "trash")
                }
            } label: {
                Image(systemName: "ellipsis.circle")
                    .frame(width: 44, height: 44)
                    .contentShape(Rectangle())
            }
            .accessibilityLabel(String(localized: "Actions", locale: store.config.locale))
            .accessibilityIdentifier("sourceChooser.actions.\(source.id.uuidString)")
        }
    }
}

private extension RegisteredSourceStatus {
    func localizedLabel(locale: Locale) -> String {
        switch self {
        case .unknown: String(localized: "Not checked", locale: locale)
        case .available: String(localized: "Available", locale: locale)
        case .offline: String(localized: "Offline", locale: locale)
        case .authenticationRequired: String(localized: "Sign-in required", locale: locale)
        case .permissionRevoked: String(localized: "Permission required", locale: locale)
        case .providerUnavailable: String(localized: "Provider unavailable", locale: locale)
        }
    }

    var systemImage: String {
        switch self {
        case .unknown: "questionmark.circle"
        case .available: "checkmark.circle"
        case .offline: "wifi.slash"
        case .authenticationRequired: "person.crop.circle.badge.exclamationmark"
        case .permissionRevoked: "lock.trianglebadge.exclamationmark"
        case .providerUnavailable: "exclamationmark.icloud"
        }
    }

    var tint: Color {
        switch self {
        case .available: .green
        case .unknown: .secondary
        case .offline, .authenticationRequired, .permissionRevoked, .providerUnavailable: .orange
        }
    }
}
