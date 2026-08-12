package io.github.mith_mmk.wml2viewer

import android.Manifest
import android.graphics.Color
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.enableEdgeToEdge
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.core.view.WindowCompat
import androidx.lifecycle.ViewModelProvider
import androidx.lifecycle.lifecycleScope
import io.github.mith_mmk.wml2viewer.ui.MobileUiHostCallbacks
import io.github.mith_mmk.wml2viewer.ui.Wml2ViewerApp
import io.github.mith_mmk.wml2viewer.ui.model.ExportDestination
import io.github.mith_mmk.wml2viewer.ui.model.ExportFormat
import io.github.mith_mmk.wml2viewer.ui.model.ExportRequest
import io.github.mith_mmk.wml2viewer.ui.state.ViewerUiEvent
import io.github.mith_mmk.wml2viewer.ui.state.ViewerViewModel
import io.github.mith_mmk.wml2viewer.ui.theme.CinematicDarkTheme
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.launch

class Wml2ViewerActivity : ComponentActivity() {
    private var viewerViewModel: ViewerViewModel? = null
    private var pendingExportRequest: ExportRequest? = null
    private val deferredViewerEvents = ArrayDeque<ViewerUiEvent>()
    private val requestNotificationPermission = registerForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { /* Foreground work remains valid when the user declines. */ }
    private val selectExportDirectory = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        val request = pendingExportRequest
        pendingExportRequest = null
        val uri = result.data?.data
        if (result.resultCode == RESULT_OK && uri != null) {
            dispatchViewerEvent(
                ViewerUiEvent.ExportDocumentCreated(uri.toString(), request),
            )
        } else {
            dispatchViewerEvent(ViewerUiEvent.ExportDocumentCancelled)
        }
    }
    private val openDocumentTree = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        val data = result.data ?: return@registerForActivityResult
        val uri = data.data ?: return@registerForActivityResult
        val takeFlags = data.flags and (
            Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_GRANT_WRITE_URI_PERMISSION
        )
        if (takeFlags and Intent.FLAG_GRANT_READ_URI_PERMISSION == 0) return@registerForActivityResult
        dispatchViewerEvent(
            ViewerUiEvent.SafRootGranted(
                uriToken = uri.toString(),
                requestRead = true,
                requestWrite = takeFlags and Intent.FLAG_GRANT_WRITE_URI_PERMISSION != 0,
            ),
        )
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        pendingExportRequest = PendingExportState.restore(savedInstanceState)
        applyDarkSystemBars(true)
        setContent { StartupContent() }
        lifecycleScope.launch {
            val component = try {
                (application as Wml2ViewerApplication).awaitComponent()
            } catch (error: Throwable) {
                if (error is CancellationException) throw error
                setContent { StartupContent(failed = true) }
                return@launch
            }
            val viewModel = ViewModelProvider(
                this@Wml2ViewerActivity,
                component.viewerViewModelFactory,
            )[ViewerViewModel::class.java]
            attachViewModel(viewModel)
            setContent {
                Wml2ViewerApp(
                    viewModel = viewModel,
                    hostCallbacks = MobileUiHostCallbacks(
                        requestSafRoot = ::requestSafRoot,
                        applyEdgeToEdge = ::applyEdgeToEdge,
                        applyDarkSystemBars = ::applyDarkSystemBars,
                        requestTransferNotifications = ::requestTransferNotifications,
                        requestCreateExportDocument = ::requestCreateExportDocument,
                    ),
                )
            }
        }
    }

    override fun onSaveInstanceState(outState: Bundle) {
        PendingExportState.save(outState, pendingExportRequest)
        super.onSaveInstanceState(outState)
    }

    private fun requestSafRoot() {
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION or
                    Intent.FLAG_GRANT_PREFIX_URI_PERMISSION,
            )
        }
        openDocumentTree.launch(intent)
    }

    private fun applyEdgeToEdge(enabled: Boolean) {
        WindowCompat.setDecorFitsSystemWindows(window, !enabled)
    }

    private fun applyDarkSystemBars(dark: Boolean) {
        enableEdgeToEdge(
            statusBarStyle = if (dark) {
                SystemBarStyle.dark(Color.TRANSPARENT)
            } else {
                SystemBarStyle.light(Color.TRANSPARENT, Color.TRANSPARENT)
            },
            navigationBarStyle = if (dark) {
                SystemBarStyle.dark(Color.rgb(9, 12, 15))
            } else {
                val lightNavigation = Color.rgb(247, 249, 254)
                SystemBarStyle.light(lightNavigation, lightNavigation)
            },
        )
    }

    private fun requestTransferNotifications() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
            checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            requestNotificationPermission.launch(Manifest.permission.POST_NOTIFICATIONS)
        }
    }

    private fun requestCreateExportDocument(request: ExportRequest) {
        if (pendingExportRequest != null) return
        pendingExportRequest = request
        val intent = Intent(Intent.ACTION_OPEN_DOCUMENT_TREE).apply {
            addFlags(
                Intent.FLAG_GRANT_READ_URI_PERMISSION or
                    Intent.FLAG_GRANT_WRITE_URI_PERMISSION,
            )
        }
        selectExportDirectory.launch(intent)
    }

    private fun dispatchViewerEvent(event: ViewerUiEvent) {
        val viewModel = viewerViewModel
        if (viewModel == null) {
            deferredViewerEvents.addLast(event)
        } else {
            viewModel.onEvent(event)
        }
    }

    private fun attachViewModel(viewModel: ViewerViewModel) {
        viewerViewModel = viewModel
        while (deferredViewerEvents.isNotEmpty()) {
            viewModel.onEvent(deferredViewerEvents.removeFirst())
        }
    }
}

internal object PendingExportState {
    private const val FORMAT = "pending_export_format"
    private const val QUALITY = "pending_export_quality"
    private const val FILE_NAME = "pending_export_file_name"
    private const val DESTINATION = "pending_export_destination"

    fun save(state: Bundle, request: ExportRequest?) {
        if (request == null) return
        state.putString(FORMAT, request.format.name)
        state.putInt(QUALITY, request.quality)
        state.putString(FILE_NAME, request.fileName)
        state.putString(DESTINATION, request.destination.name)
    }

    fun restore(state: Bundle?): ExportRequest? {
        state ?: return null
        val format = state.getString(FORMAT)?.let { runCatching { ExportFormat.valueOf(it) }.getOrNull() }
            ?: return null
        val destination = state.getString(DESTINATION)
            ?.let { runCatching { ExportDestination.valueOf(it) }.getOrNull() }
            ?: return null
        val quality = state.getInt(QUALITY, -1).takeIf { it in 0..100 } ?: return null
        val fileName = state.getString(FILE_NAME)?.takeIf { it.isNotBlank() } ?: return null
        return ExportRequest(format, quality, fileName, destination)
    }
}

@Composable
private fun StartupContent(failed: Boolean = false) {
    CinematicDarkTheme {
        val description = stringResource(
            if (failed) R.string.app_start_failed else R.string.app_starting,
        )
        Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
            if (failed) {
                Text(description, color = MaterialTheme.colorScheme.error)
            } else {
                CircularProgressIndicator(
                    Modifier.semantics { contentDescription = description },
                )
            }
        }
    }
}
