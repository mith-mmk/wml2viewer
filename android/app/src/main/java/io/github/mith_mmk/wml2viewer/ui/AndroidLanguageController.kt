package io.github.mith_mmk.wml2viewer.ui

import android.app.Activity
import android.app.LocaleManager
import android.content.Context
import android.content.ContextWrapper
import android.content.res.Configuration
import android.content.res.Resources
import android.os.Build
import android.os.LocaleList
import io.github.mith_mmk.wml2viewer.ui.model.LanguagePreference

/** Applies the DataStore-owned preference without persisting a second locale value. */
object AndroidLanguageController {
    fun languageTags(preference: LanguagePreference): String = preference.tag.orEmpty()

    fun apply(context: Context, preference: LanguagePreference) {
        val tags = languageTags(preference)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            val localeManager = context.getSystemService(LocaleManager::class.java)
            val desired = LocaleList.forLanguageTags(tags)
            if (localeManager.applicationLocales.toLanguageTags() != desired.toLanguageTags()) {
                localeManager.applicationLocales = desired
            }
            return
        }
        applyLegacy(context, tags)
    }

    @Suppress("DEPRECATION")
    private fun applyLegacy(context: Context, tags: String) {
        val desired = if (tags.isBlank()) {
            Resources.getSystem().configuration.locales
        } else {
            LocaleList.forLanguageTags(tags)
        }
        if (context.resources.configuration.locales.toLanguageTags() == desired.toLanguageTags()) {
            return
        }
        val configuration = Configuration(context.resources.configuration).apply {
            setLocales(desired)
        }
        // API 29–32 have no per-app LocaleManager. Recreate after replacing the scoped resources
        // so every Compose resource lookup observes the same locale configuration.
        context.resources.updateConfiguration(configuration, context.resources.displayMetrics)
        context.applicationContext.resources.updateConfiguration(
            configuration,
            context.applicationContext.resources.displayMetrics,
        )
        context.findActivity()?.recreate()
    }

    private tailrec fun Context.findActivity(): Activity? = when (this) {
        is Activity -> this
        is ContextWrapper -> baseContext.findActivity()
        else -> null
    }
}
