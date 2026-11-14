package io.github.naighes.scout4all

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.text.AnnotatedString
import java.time.LocalDate
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.RobolectricTestRunner

/** The analysis of a team fits every screen: its pages may scroll, no text
 * cut, the days to choose too. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class AnalysisLayoutTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }
    }

    @Test
    fun theAnalysisOfTheTeam() {
        val team = saveNationalTeam()
        winMatch(team, "Cortona")
        winMatch(team, "Arezzo")
        rule.setContent { Scout4AllTheme { AnalysisScreen(team, onBack = {}) } }
        rule.waitForText(string(R.string.last_matches, 5))
        // the last matches: the period of the test doesn't depend on today
        click(string(R.string.last_matches, 5))
        rule.waitForText(string(R.string.matches_count, 2), substring = true)
        val pages = listOf(R.string.stats_summary, R.string.stats_trend, R.string.stats_detail)
        val misfits = pages.associate { page ->
            click(string(page))
            rule.shot("analysis-${string(page)}-$screen")
            string(page) to rule.misfits(scrolls = true) + rule.inaccessible()
        }
        assertEquals(pages.associate { string(it) to emptyList<String>() }, misfits)
        click(string(R.string.between))
        rule.waitForText(string(R.string.from_day))
        rule.shot("analysis-between-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
    }

    private fun click(text: String) {
        rule.onNode(hasText(text) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForIdle()
    }
}

/** The days of the period typed, the filters of the situation. */
@RunWith(RobolectricTestRunner::class)
class AnalysisTest {
    @get:Rule val rule = createComposeRule()

    @Test
    fun theDaysTyped() {
        assertEquals(LocalDate.of(2026, 10, 25), dayOf("20261025"))
        // not whole, not a day
        assertNull(dayOf("2026102"))
        assertNull(dayOf("20261332"))
        assertEquals("2026-10-25", DayMask.filter(AnnotatedString("20261025")).text.text)
        assertEquals("2026-1", DayMask.filter(AnnotatedString("20261")).text.text)
        val mapping = DayMask.filter(AnnotatedString("20261025")).offsetMapping
        assertEquals(10, mapping.originalToTransformed(8))
        assertEquals(8, mapping.transformedToOriginal(10))
        assertEquals(4, mapping.transformedToOriginal(5))
    }

    /** The matches between two days typed: the ones of October, then none
     * in a November with no match. */
    @Test
    fun thePeriodBetweenTwoDays() {
        val team = saveNationalTeam()
        winMatch(team, "Cortona")
        rule.setContent { Scout4AllTheme { AnalysisScreen(team, onBack = {}) } }
        rule.waitForText(string(R.string.between))
        rule.onNode(hasText(string(R.string.between)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.from_day))
        days("20261001", "20261031")
        rule.waitForText("2026-10-01 → 2026-10-31", substring = true)
        rule.waitForText(string(R.string.matches_count, 1), substring = true)
        days("20261101", "20261130")
        rule.waitForText(string(R.string.no_matches_period))
        // a day that isn't: the period stays
        days("20261131", "20261130")
        rule.waitForText(string(R.string.not_a_day))
        rule.waitForText("2026-11-01 → 2026-11-30", substring = true)
    }

    /** The reception of the rally chosen by its group: the chip says it. */
    @Test
    fun aFilterOfTheSituation() {
        val team = saveNationalTeam()
        winMatch(team, "Cortona")
        rule.setContent { Scout4AllTheme { AnalysisScreen(team, onBack = {}) } }
        rule.waitForText(string(R.string.rally_reception))
        rule.onNode(hasText(string(R.string.rally_reception)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        // `!` among the bad ones
        rule.waitForText(string(R.string.negative_group) + " !-/")
        rule.onAllNodesWithText(string(R.string.negative_group) + " !-/")[0].performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.rally_reception) + " !-/")
    }

    /** Two rotations at a time: the menu stays open, the chip names both. */
    @Test
    fun twoRotationsAtATime() {
        val team = saveNationalTeam()
        winMatch(team, "Cortona")
        rule.setContent { Scout4AllTheme { AnalysisScreen(team, onBack = {}) } }
        rule.waitForText(string(R.string.filter_rotation))
        rule.onNode(hasText(string(R.string.filter_rotation)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText("S1")
        rule.onNode(hasText("S1") and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.onNode(hasText("S3") and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.filter_rotation) + " S1 S3")
    }

    private fun days(from: String, to: String) {
        val fields = rule.onAllNodes(hasSetTextAction())
        fields[0].performTextReplacement(from)
        fields[1].performTextReplacement(to)
        rule.waitForIdle()
    }
}
