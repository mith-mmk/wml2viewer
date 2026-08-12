package io.github.mith_mmk.wml2viewer.ui.model

import androidx.compose.runtime.Immutable

enum class DeviceClass { COMPACT, EXPANDED }

enum class MobileScreen { VIEWER, FILER, SETTINGS }

enum class SettingsCategory {
    VIEWING,
    MANGA,
    TOUCH,
    FILER,
    CODECS,
    LANGUAGE_AND_APPEARANCE,
    CACHE,
    ABOUT,
}

enum class ViewerAction(val wireName: String) {
    NONE("None"),
    PREVIOUS_IMAGE("PrevImage"),
    NEXT_IMAGE("NextImage"),
    FIRST_IMAGE("FirstImage"),
    LAST_IMAGE("LastImage"),
    ZOOM_IN("ZoomIn"),
    ZOOM_OUT("ZoomOut"),
    ZOOM_RESET("ZoomReset"),
    TOGGLE_FIT_MODE("ToggleFitMode"),
    TOGGLE_ANIMATION("ToggleAnimation"),
    TOGGLE_GRAYSCALE("ToggleGrayscale"),
    TOGGLE_MANGA_MODE("ToggleMangaMode"),
    OPEN_FILER("ToggleFiler"),
    OPEN_SETTINGS("ToggleSettings"),
    OPEN_SUBFILER("ToggleSubfiler"),
    OPEN_CONTEXT_MENU("OpenContextMenu"),
    EXPORT("Export"),
    RELOAD("Reload"),
}

enum class ExportFormat(
    val mimeType: String,
    val extension: String,
) {
    PNG("image/png", "png"),
    JPEG("image/jpeg", "jpg"),
    WEBP_LOSSY("image/webp", "webp"),
    WEBP_LOSSLESS("image/webp", "webp"),
}

enum class ExportDestination { CURRENT_DIRECTORY, SYSTEM_PICKER }

@Immutable
data class ExportRequest(
    val format: ExportFormat,
    val quality: Int,
    val fileName: String,
    val destination: ExportDestination,
)

enum class TapZone(val row: Int, val column: Int) {
    TOP_LEFT(0, 0),
    TOP_CENTER(0, 1),
    TOP_RIGHT(0, 2),
    MIDDLE_LEFT(1, 0),
    CENTER(1, 1),
    MIDDLE_RIGHT(1, 2),
    BOTTOM_LEFT(2, 0),
    BOTTOM_CENTER(2, 1),
    BOTTOM_RIGHT(2, 2),
    ;

    companion object {
        fun at(row: Int, column: Int): TapZone =
            entries.first { it.row == row && it.column == column }
    }
}

@Immutable
data class TouchMapConfig(
    val bindings: Map<TapZone, ViewerAction> = defaultBindings(),
) {
    fun actionFor(zone: TapZone): ViewerAction = bindings[zone] ?: ViewerAction.NONE

    fun withBinding(zone: TapZone, action: ViewerAction): TouchMapConfig =
        copy(bindings = bindings + (zone to action))

    companion object {
        fun defaultBindings(): Map<TapZone, ViewerAction> = mapOf(
            TapZone.TOP_LEFT to ViewerAction.PREVIOUS_IMAGE,
            TapZone.TOP_CENTER to ViewerAction.OPEN_FILER,
            TapZone.TOP_RIGHT to ViewerAction.NEXT_IMAGE,
            TapZone.MIDDLE_LEFT to ViewerAction.PREVIOUS_IMAGE,
            TapZone.CENTER to ViewerAction.OPEN_SETTINGS,
            TapZone.MIDDLE_RIGHT to ViewerAction.NEXT_IMAGE,
            TapZone.BOTTOM_LEFT to ViewerAction.PREVIOUS_IMAGE,
            TapZone.BOTTOM_CENTER to ViewerAction.OPEN_SUBFILER,
            TapZone.BOTTOM_RIGHT to ViewerAction.NEXT_IMAGE,
        )
    }
}

@Immutable
data class GestureSettings(
    val swipeEnabled: Boolean = false,
    val pinchZoom: Boolean = true,
    val pan: Boolean = true,
    val doubleTapAction: ViewerAction = ViewerAction.TOGGLE_FIT_MODE,
    val longPressAction: ViewerAction = ViewerAction.OPEN_CONTEXT_MENU,
)

enum class MangaLayoutMode { AUTO, SINGLE, SPREAD }

enum class ReadingDirection { RIGHT_TO_LEFT, LEFT_TO_RIGHT }

@Immutable
data class MangaSettings(
    val layoutMode: MangaLayoutMode = MangaLayoutMode.AUTO,
    val readingDirection: ReadingDirection = ReadingDirection.RIGHT_TO_LEFT,
    val singleCover: Boolean = true,
    val divider: Boolean = false,
    val prefetchSpreads: Int = 1,
)

enum class CodecPolicy {
    DEFAULT,
    INTERNAL_FIRST,
    OS_FIRST,
    INTERNAL_ONLY,
    OS_ONLY,
}

enum class CodecFormat {
    JPEG,
    PNG,
    GIF,
    WEBP,
    BMP,
    ICO,
    HEIF,
    AVIF,
    DNG,
}

@Immutable
data class CodecSettings(
    val defaultPolicy: CodecPolicy = CodecPolicy.INTERNAL_FIRST,
    val overrides: Map<CodecFormat, CodecPolicy> = emptyMap(),
) {
    fun policyFor(format: CodecFormat): CodecPolicy = overrides[format] ?: CodecPolicy.DEFAULT

    fun withOverride(format: CodecFormat, policy: CodecPolicy): CodecSettings = copy(
        overrides = if (policy == CodecPolicy.DEFAULT) overrides - format
        else overrides + (format to policy),
    )
}

enum class LanguagePreference(val tag: String?) {
    SYSTEM(null),
    ENGLISH("en"),
    JAPANESE("ja"),
}

enum class TextScale { SMALL, MEDIUM, LARGE }

enum class ThemeMode { CINEMATIC_DARK, LIGHT, SYSTEM }

enum class DisplayFit { CONTAIN, WIDTH, HEIGHT, ORIGINAL }

@Immutable
data class ViewingSettings(
    val edgeToEdge: Boolean = true,
    val keepScreenOn: Boolean = false,
    val fit: DisplayFit = DisplayFit.CONTAIN,
    val showTopChrome: Boolean = true,
    val showFilmstrip: Boolean = true,
)

enum class FilerSortOrder { NAME_ASCENDING, NAME_DESCENDING, MODIFIED_DESCENDING }

@Immutable
data class FilerSettings(
    val tabletFilmstripPinned: Boolean = false,
    val showHiddenFiles: Boolean = false,
    val sortOrder: FilerSortOrder = FilerSortOrder.NAME_ASCENDING,
    val rememberLastLocation: Boolean = true,
)

@Immutable
data class CacheSettings(
    val automaticLimit: Boolean = true,
    val manualLimitMiB: Int = 512,
)

@Immutable
data class MobileViewerSettings(
    val viewing: ViewingSettings = ViewingSettings(),
    val touchMap: TouchMapConfig = TouchMapConfig(),
    val gestures: GestureSettings = GestureSettings(),
    val manga: MangaSettings = MangaSettings(),
    val filer: FilerSettings = FilerSettings(),
    val codecs: CodecSettings = CodecSettings(),
    val language: LanguagePreference = LanguagePreference.SYSTEM,
    val theme: ThemeMode = ThemeMode.CINEMATIC_DARK,
    val dynamicColor: Boolean = false,
    val textScale: TextScale = TextScale.MEDIUM,
    val cache: CacheSettings = CacheSettings(),
)

enum class SourceKind { LOCAL, SMB }

@Immutable
data class FilerEntryUi(
    val id: String,
    val name: String,
    val isContainer: Boolean,
    val subtitle: String? = null,
    val capabilities: FilerCapabilitiesUi = FilerCapabilitiesUi(),
    val managedSourceId: String? = null,
    val credentialReentryRequired: Boolean = false,
    val canForgetSource: Boolean = false,
)

@Immutable
data class FilerCapabilitiesUi(
    val canCreate: Boolean = false,
    val canCopy: Boolean = false,
    val canMove: Boolean = false,
    val canRename: Boolean = false,
    val canTrash: Boolean = false,
    val canDeletePermanently: Boolean = false,
)

@Immutable
data class BreadcrumbUi(
    val id: String,
    val label: String,
    val sourceRoot: Boolean = false,
)

enum class FilerOperationType { CREATE_FOLDER, COPY, MOVE, RENAME, DELETE }

@Immutable
data class FilerOperationRequest(
    val type: FilerOperationType,
    val entryId: String? = null,
    val destinationId: String? = null,
    val name: String? = null,
    val allowPermanentDelete: Boolean = false,
)

@Immutable
data class PendingTransferUi(
    val type: FilerOperationType,
    val entryId: String,
    val entryName: String,
)

enum class CollisionResolution { REPLACE, KEEP_BOTH, SKIP }

@Immutable
data class PendingCollisionUi(
    val operationId: String,
    val displayName: String,
    val allowReplace: Boolean = true,
    val supportsApplyToAll: Boolean = false,
)

data class SmbConnectionInput(
    val server: String,
    val port: Int = 445,
    val share: String = "",
    val username: String = "",
    val password: CharArray = charArrayOf(),
    val domain: String = "",
    val guest: Boolean = false,
    val requireEncryption: Boolean = false,
    val setupId: String? = null,
) {
    /** Call from a finally block immediately after handing credentials to the platform layer. */
    fun clearPassword() {
        password.fill('\u0000')
    }

    override fun toString(): String =
        "SmbConnectionInput(server=[REDACTED], port=$port, share=${if (share.isEmpty()) "[NONE]" else "[SET]"}, " +
            "username=${if (username.isEmpty()) "[NONE]" else "[SET]"}, " +
            "password=[REDACTED], domain=${if (domain.isEmpty()) "[NONE]" else "[SET]"}, " +
            "guest=$guest, requireEncryption=$requireEncryption, setupId=${setupId?.let { "[SET]" } ?: "[NONE]"})"
}

data class SmbCredentialInput(
    val sourceId: String,
    val password: CharArray,
) {
    fun clearPassword() {
        password.fill('\u0000')
    }

    override fun toString(): String = "SmbCredentialInput(sourceId=[REDACTED], password=[REDACTED])"
}

@Immutable
data class SmbSecurityStatusUi(
    val signingActive: Boolean?,
    val encryptionActive: Boolean?,
    val dialect: String?,
)

@Immutable
data class FilmstripItemUi(
    val id: String,
    val label: String,
    val selected: Boolean,
)

@Immutable
data class MangaPageRef(
    val id: String,
    val sourceBoundary: String,
    val portrait: Boolean,
)
