package io.github.mith_mmk.wml2viewer.ui.theme

import android.os.Build
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.dynamicDarkColorScheme
import androidx.compose.material3.dynamicLightColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.sp
import io.github.mith_mmk.wml2viewer.ui.model.TextScale
import io.github.mith_mmk.wml2viewer.ui.model.ThemeMode

private val CinematicDarkColors = darkColorScheme(
    primary = Color(0xFF72D2FF),
    onPrimary = Color(0xFF001F2A),
    primaryContainer = Color(0xFF11384A),
    onPrimaryContainer = Color(0xFFC2EAFF),
    secondary = Color(0xFFB9A1FF),
    onSecondary = Color(0xFF24134F),
    secondaryContainer = Color(0xFF39276A),
    onSecondaryContainer = Color(0xFFE5DCFF),
    tertiary = Color(0xFF4CE0B4),
    onTertiary = Color(0xFF00382A),
    background = Color(0xFF07090D),
    onBackground = Color(0xFFE7EBF2),
    surface = Color(0xFF10141C),
    onSurface = Color(0xFFE7EBF2),
    surfaceVariant = Color(0xFF1A202B),
    onSurfaceVariant = Color(0xFFBFC7D5),
    outline = Color(0xFF657080),
    error = Color(0xFFFFB4AB),
    onError = Color(0xFF690005),
)

private val CinematicLightColors = lightColorScheme(
    primary = Color(0xFF006685),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFBDE9FF),
    onPrimaryContainer = Color(0xFF001F2A),
    secondary = Color(0xFF5E4B91),
    onSecondary = Color.White,
    background = Color(0xFFF7F9FE),
    onBackground = Color(0xFF181C20),
    surface = Color(0xFFFFFFFF),
    onSurface = Color(0xFF181C20),
    surfaceVariant = Color(0xFFE0E7EE),
    onSurfaceVariant = Color(0xFF41484D),
)

@Composable
fun CinematicDarkTheme(
    textScale: TextScale = TextScale.MEDIUM,
    themeMode: ThemeMode = ThemeMode.CINEMATIC_DARK,
    dynamicColor: Boolean = false,
    content: @Composable () -> Unit,
) {
    val context = LocalContext.current
    val useDarkColors = themeMode == ThemeMode.CINEMATIC_DARK ||
        (themeMode == ThemeMode.SYSTEM && isSystemInDarkTheme())
    val colors = if (dynamicColor && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
        if (useDarkColors) dynamicDarkColorScheme(context) else dynamicLightColorScheme(context)
    } else if (useDarkColors) {
        CinematicDarkColors
    } else {
        CinematicLightColors
    }
    val scale = when (textScale) {
        TextScale.SMALL -> 0.9f
        TextScale.MEDIUM -> 1f
        TextScale.LARGE -> 1.15f
    }
    MaterialTheme(
        colorScheme = colors,
        typography = MaterialTheme.typography.copy(
            headlineSmall = TextStyle(
                fontFamily = FontFamily.SansSerif,
                fontWeight = FontWeight.SemiBold,
                fontSize = 22.sp * scale,
                letterSpacing = 0.2.sp,
            ),
            titleMedium = TextStyle(
                fontFamily = FontFamily.SansSerif,
                fontWeight = FontWeight.SemiBold,
                fontSize = 16.sp * scale,
                letterSpacing = 0.15.sp,
            ),
            bodyMedium = TextStyle(
                fontFamily = FontFamily.SansSerif,
                fontWeight = FontWeight.Normal,
                fontSize = 14.sp * scale,
                letterSpacing = 0.1.sp,
            ),
            labelLarge = TextStyle(
                fontFamily = FontFamily.SansSerif,
                fontWeight = FontWeight.SemiBold,
                fontSize = 14.sp * scale,
                letterSpacing = 0.4.sp,
            ),
        ),
        content = content,
    )
}
