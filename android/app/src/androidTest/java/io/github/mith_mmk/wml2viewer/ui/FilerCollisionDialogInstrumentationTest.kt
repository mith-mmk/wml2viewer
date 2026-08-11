package io.github.mith_mmk.wml2viewer.ui

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.test.core.app.ApplicationProvider
import io.github.mith_mmk.wml2viewer.R
import io.github.mith_mmk.wml2viewer.ui.components.FilerCollisionDialog
import io.github.mith_mmk.wml2viewer.ui.model.PendingCollisionUi
import org.junit.Rule
import org.junit.Test
import org.junit.Assert.assertTrue

class FilerCollisionDialogInstrumentationTest {
    @get:Rule
    val compose = createComposeRule()

    @Test
    fun externalTreeExportHidesNonRecoverableReplaceChoice() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        compose.setContent {
            FilerCollisionDialog(
                collision = PendingCollisionUi(
                    operationId = "external-export",
                    displayName = "page.png",
                    allowReplace = false,
                ),
                onResolve = { _, _, _ -> },
            )
        }

        assertTrue(
            compose.onAllNodesWithText(context.getString(R.string.filer_collision_replace))
                .fetchSemanticsNodes().isEmpty(),
        )
        compose.onNodeWithText(context.getString(R.string.filer_collision_keep_both)).assertIsDisplayed()
        compose.onNodeWithText(context.getString(R.string.filer_collision_skip)).assertIsDisplayed()
    }
}
