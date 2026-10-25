package io.github.naighes.scout4all

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performSemanticsAction
import io.github.naighes.scout4all.core.EvalEnum
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** The statistics of a match fit every screen: they may scroll, no text
 * cut. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class StatsLayoutTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }
    }

    @Test
    fun theStatisticsOfTheMatch() {
        val team = saveNationalTeam()
        val match = startFirstSet(team)
        serve(team, match, EvalEnum.PERFECT, EvalEnum.POSITIVE)
        rule.setContent { Scout4AllTheme { MatchStatsScreen(team, match, null, onBack = {}) } }
        rule.waitForText(string(R.string.stats_summary))
        val pages = listOf(R.string.stats_summary, R.string.stats_distribution, R.string.stats_errors, R.string.stats_trend, R.string.stats_detail)
        val misfits = pages.associate { page ->
            // the tab may be out of its row
            rule.onNode(hasText(string(page)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
            rule.waitForIdle()
            rule.shot("stats-${string(page)}-$screen")
            string(page) to rule.misfits(scrolls = true)
        }
        assertEquals(pages.associate { string(it) to emptyList<String>() }, misfits)
    }
}
