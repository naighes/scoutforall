package io.github.naighes.scout4all

import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.luminance
import org.junit.Assert.assertEquals
import org.junit.Test

/** The contrast of the colors of the theme, as WCAG measures it: the app is
 * used on the court side, in the sun too. A text at 4.5:1 at least (AA),
 * the ones of the scouting at 7:1 (AAA); lines and shapes at 3:1. What
 * can't be touched now is dimmed: WCAG doesn't count it. */
class ContrastTest {
    private val c = Scout4AllColors

    // the colors of the grades, from red to green
    private val grades = (0..20).map { gradeColor(it / 20.0) }

    private fun ratio(a: Color, b: Color): Double {
        val (light, dark) = listOf(a.luminance(), b.luminance()).sortedDescending()
        return (light + 0.05) / (dark + 0.05)
    }

    /** The pairs below `least`, with their ratio. */
    private fun below(least: Double, pairs: List<Triple<String, Color, Color>>) =
        pairs.map { (name, a, b) -> name to ratio(a, b) }.filter { it.second < least }
            .map { (name, r) -> "$name ${"%.2f".format(r)}" }

    @Test
    fun everyTextIsReadable() = assertEquals(
        emptyList<String>(),
        below(
            4.5,
            listOf(
                Triple("text", c.onSurface, c.surface),
                Triple("text on a tile", c.onSurface, c.surfaceVariant),
                Triple("secondary text", c.onSurfaceVariant, c.surface),
                Triple("secondary text on a tile, positions on court", c.onSurfaceVariant, c.surfaceVariant),
                Triple("amber text", c.primary, c.surface),
                Triple("amber text on a tile", c.primary, c.surfaceVariant),
                Triple("on an amber button", c.onPrimary, c.primary),
                Triple("tonal button", c.onSecondaryContainer, c.secondaryContainer),
                Triple("primary container", c.onPrimaryContainer, c.primaryContainer),
                Triple("error", c.error, c.surface),
            ) + grades.flatMap {
                listOf(
                    Triple("grade $it", it, c.surface),
                    Triple("grade $it on a tile", it, c.surfaceVariant),
                    Triple("on grade $it", Color.Black, it),
                )
            },
        ),
    )

    @Test
    fun theScoutingIsReadableAtAGlance() = assertEquals(
        emptyList<String>(),
        below(
            7.0,
            listOf(
                Triple("score, team names", c.onSurface, c.surface),
                Triple("names on court", c.onSurface, c.surfaceVariant),
                Triple("number on the jersey", c.surface, c.onSurface),
                Triple("number on the jersey of the libero", c.onSurface, c.background),
                Triple("phase, hint, last event", c.onSurfaceVariant, c.surface),
                Triple("break", c.primary, c.surface),
                Triple("evaluations, half-moon, badges", c.onPrimary, c.primary),
                Triple("events of the rally", c.onSecondaryContainer, c.secondaryContainer),
                Triple("efficiency without a grade", c.onSurface, c.surfaceVariant),
            ) + grades.map { Triple("efficiency of grade $it", Color.Black, it) },
        ),
    )

    @Test
    fun linesAndShapesStandOut() = assertEquals(
        emptyList<String>(),
        below(
            3.0,
            listOf(
                Triple("ball on court", c.primary, c.surfaceVariant),
                Triple("axis", c.outline, c.surface),
                Triple("jersey on court", c.onSurface, c.surfaceVariant),
            ) + setColors.flatMap {
                listOf(Triple("set line $it", it, c.surface), Triple("set line $it on a tile", it, c.surfaceVariant))
            },
        ),
    )
}
