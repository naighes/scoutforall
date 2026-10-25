package io.github.naighes.scout4all

import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color

// The colors of the site (web/src/assets/css/global.css): always dark, amber
// only for what the scout acts on. The grey of the secondary text lighter
// than the site's (#9F9F9F), to read it at 7:1 on the court side, in the
// sun too (`ContrastTest`).
private val Surface0 = Color(0xFF121212)
private val Surface1 = Color(0xFF18181B)
private val Surface3 = Color(0xFF2F2F31)
private val ForegroundLight = Color(0xFFEEEEEE)
private val ForegroundDark = Color(0xFFA8A8A8)
private val Amber = Color(0xFFFCD34D)

// a match won or lost: Tailwind green and red 400, as the amber is Tailwind
val Won = Color(0xFF4ADE80)
val Lost = Color(0xFFF87171)
val Fair = Color(0xFFFACC15)

internal val Scout4AllColors = darkColorScheme(
    primary = Amber,
    onPrimary = Color.Black,
    primaryContainer = Surface3,
    onPrimaryContainer = Amber,
    secondary = ForegroundLight,
    onSecondary = Color.Black,
    secondaryContainer = Surface3,
    onSecondaryContainer = ForegroundLight,
    background = Surface1,
    onBackground = ForegroundLight,
    surface = Surface1,
    onSurface = ForegroundLight,
    surfaceVariant = Surface3,
    onSurfaceVariant = ForegroundDark,
    surfaceContainerLowest = Surface0,
    surfaceContainerLow = Surface0,
    surfaceContainer = Surface1,
    surfaceContainerHigh = Surface3,
    surfaceContainerHighest = Surface3,
    outline = ForegroundDark,
    outlineVariant = Surface3,
)

/** Roboto, the font of the system. */
@Composable
fun Scout4AllTheme(content: @Composable () -> Unit) =
    MaterialTheme(colorScheme = Scout4AllColors, content = content)
