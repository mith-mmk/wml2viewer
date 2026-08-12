package io.github.mith_mmk.wml2viewer.ui.components

import androidx.compose.foundation.Image
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.calculatePan
import androidx.compose.foundation.gestures.calculateZoom
import androidx.compose.foundation.gestures.detectHorizontalDragGestures
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ColorFilter
import androidx.compose.ui.graphics.ColorMatrix
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.layout.boundsInWindow
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalLayoutDirection
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import io.github.mith_mmk.wml2viewer.R
import io.github.mith_mmk.wml2viewer.ui.model.GestureSettings
import io.github.mith_mmk.wml2viewer.ui.model.TapZone
import io.github.mith_mmk.wml2viewer.ui.model.ViewerAction
import io.github.mith_mmk.wml2viewer.ui.state.MobileViewerUiState
import io.github.mith_mmk.wml2viewer.ui.state.UiError
import io.github.mith_mmk.wml2viewer.ui.state.UiErrorCode
import io.github.mith_mmk.wml2viewer.ui.state.ViewerPageFrameUi
import io.github.mith_mmk.wml2viewer.ui.touch.GestureArbiter
import io.github.mith_mmk.wml2viewer.ui.touch.GestureIntent
import io.github.mith_mmk.wml2viewer.ui.touch.TapZoneResolver
import io.github.mith_mmk.wml2viewer.ui.touch.TouchInsets
import kotlin.math.abs
import kotlin.math.roundToInt

@Composable
fun ViewerSurface(
    state: MobileViewerUiState,
    gestureSettings: GestureSettings,
    onZoneTap: (TapZone) -> Unit,
    onAction: (ViewerAction) -> Unit,
    onTransform: (panX: Float, panY: Float, zoomChange: Float) -> Unit,
    onViewportSizeChanged: (IntSize) -> Unit = {},
    modifier: Modifier = Modifier,
) {
    var size by remember { mutableStateOf(IntSize.Zero) }
    var boundsInWindow by remember { mutableStateOf(Rect.Zero) }
    val density = LocalDensity.current
    val layoutDirection = LocalLayoutDirection.current
    val hostView = LocalView.current
    val safeDrawing = WindowInsets.safeDrawing
    val localTouchInsets = if (
        boundsInWindow.width > 0f && boundsInWindow.height > 0f &&
        hostView.width > 0 && hostView.height > 0
    ) {
        val windowLeft = safeDrawing.getLeft(density, layoutDirection).toFloat()
        val windowTop = safeDrawing.getTop(density).toFloat()
        val windowRight = safeDrawing.getRight(density, layoutDirection).toFloat()
        val windowBottom = safeDrawing.getBottom(density).toFloat()
        val safeRightEdge = hostView.width.toFloat() - windowRight
        val safeBottomEdge = hostView.height.toFloat() - windowBottom
        TouchInsets(
            left = (windowLeft - boundsInWindow.left).coerceAtLeast(0f),
            top = (windowTop - boundsInWindow.top).coerceAtLeast(0f),
            right = (boundsInWindow.right - safeRightEdge).coerceAtLeast(0f),
            bottom = (boundsInWindow.bottom - safeBottomEdge).coerceAtLeast(0f),
        )
    } else {
        TouchInsets()
    }
    val arbiter = remember { GestureArbiter() }
    val colorFilter = remember(state.grayscaleEnabled) {
        val saturation = viewerSaturation(state.grayscaleEnabled)
        if (saturation == 1f) null
        else ColorFilter.colorMatrix(
            ColorMatrix().apply { setToSaturation(saturation) },
        )
    }
    val spreadFrames = state.engine.spreadFrames
    val displayWidth = if (spreadFrames.isNotEmpty()) {
        spreadFrames.sumOf { it.frame.width }
    } else {
        state.engine.frame?.width
    }
    val displayHeight = if (spreadFrames.isNotEmpty()) {
        spreadFrames.maxOf { it.frame.height }
    } else {
        state.engine.frame?.height
    }
    val fittedImageRect = if (displayWidth != null && displayHeight != null) {
        TapZoneResolver.imageRect(
            surfaceWidth = size.width.toFloat(),
            surfaceHeight = size.height.toFloat(),
            imageWidth = displayWidth,
            imageHeight = displayHeight,
            fit = state.fitOverride ?: state.settings.viewing.fit,
            zoom = state.zoom,
            panX = state.panX,
            panY = state.panY,
        )
    } else null
    val visibleTouchRect = TapZoneResolver.visibleImageRect(
        imageRect = fittedImageRect,
        surfaceWidth = size.width.toFloat(),
        surfaceHeight = size.height.toFloat(),
        insets = localTouchInsets,
    )
    val gestureModifier = Modifier
        .pointerInput(gestureSettings.pinchZoom, gestureSettings.pan, state.zoom) {
            if (!gestureSettings.pinchZoom && !gestureSettings.pan) return@pointerInput
            awaitEachGesture {
                awaitFirstDown(requireUnconsumed = false)
                arbiter.beginPointerSequence()
                do {
                    val event = awaitPointerEvent()
                    val pointerCount = event.changes.count { it.pressed }
                    val zoomChange = event.calculateZoom()
                    val pan = event.calculatePan()
                    val pinchActive = pointerCount >= 2 && gestureSettings.pinchZoom &&
                        abs(zoomChange - 1f) > 0.001f
                    val panActive = gestureSettings.pan &&
                        (pointerCount >= 2 || state.zoom > 1f) &&
                        (abs(pan.x) > 0.01f || abs(pan.y) > 0.01f)
                    val intent = when {
                        pinchActive -> GestureIntent.PINCH
                        panActive -> GestureIntent.PAN
                        else -> null
                    }
                    if (intent != null && arbiter.accept(intent)) {
                        onTransform(
                            if (panActive) pan.x else 0f,
                            if (panActive) pan.y else 0f,
                            if (pinchActive) zoomChange else 1f,
                        )
                        event.changes.forEach { it.consume() }
                    }
                } while (event.changes.any { it.pressed })
                arbiter.endPointerSequence()
            }
        }
        .pointerInput(gestureSettings.swipeEnabled, state.zoom) {
            if (!gestureSettings.swipeEnabled || state.zoom > 1f) return@pointerInput
            var horizontalDistance = 0f
            val actionThreshold = viewConfiguration.touchSlop * 3f
            detectHorizontalDragGestures(
                onDragStart = {
                    arbiter.beginPointerSequence()
                    horizontalDistance = 0f
                    arbiter.accept(GestureIntent.SWIPE)
                },
                onHorizontalDrag = { change, amount ->
                    if (arbiter.accept(GestureIntent.SWIPE)) {
                        horizontalDistance += amount
                        change.consume()
                    }
                },
                onDragEnd = {
                    if (arbiter.current() == GestureIntent.SWIPE &&
                        abs(horizontalDistance) >= actionThreshold
                    ) {
                        onAction(
                            if (horizontalDistance < 0f) ViewerAction.NEXT_IMAGE
                            else ViewerAction.PREVIOUS_IMAGE,
                        )
                    }
                    arbiter.endPointerSequence()
                },
                onDragCancel = {
                    horizontalDistance = 0f
                    arbiter.endPointerSequence()
                },
            )
        }
        .pointerInput(
            gestureSettings.doubleTapAction,
            gestureSettings.longPressAction,
            visibleTouchRect,
            state.touchReady,
        ) {
            detectTapGestures(
                onPress = {
                    arbiter.beginPointerSequence()
                    try {
                        tryAwaitRelease()
                    } finally {
                        arbiter.endPointerSequence()
                    }
                },
                onTap = { offset ->
                    if (state.touchReady && arbiter.accept(GestureIntent.SINGLE_TAP)) {
                        TapZoneResolver.resolve(
                            x = offset.x,
                            y = offset.y,
                            rect = visibleTouchRect,
                        )?.let(onZoneTap)
                    }
                },
                onDoubleTap = {
                    if (arbiter.accept(GestureIntent.DOUBLE_TAP)) {
                        onAction(gestureSettings.doubleTapAction)
                    }
                },
                onLongPress = {
                    if (arbiter.accept(GestureIntent.LONG_PRESS)) {
                        onAction(gestureSettings.longPressAction)
                    }
                },
            )
        }

    Box(
        modifier = modifier
            .fillMaxSize()
            .background(Color.Black)
            .onGloballyPositioned { boundsInWindow = it.boundsInWindow() }
            .onSizeChanged {
                size = it
                onViewportSizeChanged(it)
            }
            .then(gestureModifier)
            .testTag("viewer-surface"),
        contentAlignment = Alignment.Center,
    ) {
        val frame = state.engine.frame
        if (spreadFrames.isNotEmpty()) {
            MangaSpreadCanvas(
                frames = spreadFrames,
                displayRect = fittedImageRect,
                divider = state.settings.manga.divider,
                colorFilter = colorFilter,
                contentDescription = stringResource(R.string.viewer_image_content_description),
                modifier = Modifier.fillMaxSize(),
            )
        } else if (frame != null) {
            Image(
                bitmap = frame,
                contentDescription = stringResource(R.string.viewer_image_content_description),
                colorFilter = colorFilter,
                contentScale = when (state.fitOverride ?: state.settings.viewing.fit) {
                    io.github.mith_mmk.wml2viewer.ui.model.DisplayFit.CONTAIN -> ContentScale.Fit
                    io.github.mith_mmk.wml2viewer.ui.model.DisplayFit.WIDTH -> ContentScale.FillWidth
                    io.github.mith_mmk.wml2viewer.ui.model.DisplayFit.HEIGHT -> ContentScale.FillHeight
                    io.github.mith_mmk.wml2viewer.ui.model.DisplayFit.ORIGINAL -> ContentScale.None
                },
                modifier = Modifier
                    .fillMaxSize()
                    .graphicsLayer(
                        scaleX = state.zoom,
                        scaleY = state.zoom,
                        translationX = state.panX,
                        translationY = state.panY,
                    ),
            )
        } else if (!state.engine.loading) {
            Text(
                text = stringResource(R.string.viewer_empty),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        if (state.engine.loading) CircularProgressIndicator()
        state.engine.error?.let { error ->
            Text(
                text = "${stringResource(R.string.error_title)}\n${error.localizedMessage()}",
                color = MaterialTheme.colorScheme.error,
                modifier = Modifier.align(Alignment.BottomCenter),
            )
        }
    }
}

@Composable
private fun MangaSpreadCanvas(
    frames: List<ViewerPageFrameUi>,
    displayRect: io.github.mith_mmk.wml2viewer.ui.touch.TouchRect?,
    divider: Boolean,
    colorFilter: ColorFilter?,
    contentDescription: String,
    modifier: Modifier = Modifier,
) {
    val dividerColor = MaterialTheme.colorScheme.outline
    Canvas(modifier.semantics { this.contentDescription = contentDescription }) {
        val rect = displayRect ?: return@Canvas
        val totalWidth = frames.sumOf { it.frame.width }.toFloat()
        if (totalWidth <= 0f) return@Canvas
        val scale = rect.width / totalWidth
        var x = rect.left
        frames.forEachIndexed { index, item ->
            val pageWidth = item.frame.width * scale
            val pageHeight = item.frame.height * scale
            val pageTop = rect.top + (rect.height - pageHeight) / 2f
            drawImage(
                image = item.frame,
                srcOffset = IntOffset.Zero,
                srcSize = IntSize(item.frame.width, item.frame.height),
                dstOffset = IntOffset(x.roundToInt(), pageTop.roundToInt()),
                dstSize = IntSize(
                    pageWidth.roundToInt().coerceAtLeast(1),
                    pageHeight.roundToInt().coerceAtLeast(1),
                ),
                colorFilter = colorFilter,
            )
            x += pageWidth
            if (divider && index < frames.lastIndex) {
                drawLine(
                    color = dividerColor,
                    start = Offset(x, rect.top),
                    end = Offset(x, rect.bottom),
                    strokeWidth = 1.dp.toPx(),
                )
            }
        }
    }
}

@Composable
private fun UiError.localizedMessage(): String = when (code) {
    UiErrorCode.INVALID_HANDLE -> stringResource(R.string.error_invalid_handle)
    UiErrorCode.INVALID_REQUEST -> stringResource(R.string.error_invalid_request)
    UiErrorCode.STALE_REQUEST -> stringResource(R.string.error_stale_request)
    UiErrorCode.CANCELLED -> stringResource(R.string.error_cancelled)
    UiErrorCode.IO -> stringResource(R.string.error_io)
    UiErrorCode.DECODE -> stringResource(R.string.error_decode)
    UiErrorCode.ENCODE -> stringResource(R.string.error_encode)
    UiErrorCode.OS_ANIMATION_UNSUPPORTED -> stringResource(R.string.error_os_animation_unsupported)
    UiErrorCode.LIMIT -> args.firstOrNull()?.toLimitLabel()?.let {
        stringResource(R.string.error_limit_with_value, it)
    } ?: stringResource(R.string.error_limit)
    UiErrorCode.AUTHENTICATION_FAILED -> stringResource(R.string.error_authentication_failed)
    UiErrorCode.ACCESS_DENIED -> stringResource(R.string.error_access_denied)
    UiErrorCode.NETWORK -> stringResource(R.string.error_network)
    UiErrorCode.INTEGRITY -> stringResource(R.string.error_integrity)
    UiErrorCode.PERMISSION_REVOKED -> stringResource(R.string.error_permission_revoked)
    UiErrorCode.KEYSTORE_INVALIDATED -> stringResource(R.string.error_keystore_invalidated)
    UiErrorCode.UNKNOWN -> stringResource(R.string.error_unknown)
}

@Composable
private fun String.toLimitLabel(): String? = when (this) {
    "width" -> stringResource(R.string.error_limit_width)
    "height" -> stringResource(R.string.error_limit_height)
    "stride" -> stringResource(R.string.error_limit_stride)
    else -> null
}
