package io.github.mith_mmk.wml2viewer.ui.components

internal fun viewerSaturation(grayscaleEnabled: Boolean): Float =
    if (grayscaleEnabled) 0f else 1f
