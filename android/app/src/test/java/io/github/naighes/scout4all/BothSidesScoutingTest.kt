package io.github.naighes.scout4all

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.EventTypeEnum
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Scouting both sides of a match from the phone: the two sides in a line,
 * the swap, the events of either side with their team. */
@RunWith(RobolectricTestRunner::class)
class BothSidesScoutingTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()
    private val match = startFirstSetBoth(team)

    private fun show() {
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = {}) } }
        rule.waitForText("Arezzo", substring = true)
    }

    private fun swap() = rule.onNode(hasContentDescription(string(R.string.swap_side)) and hasClickAction())

    /** The side the choices are for: the one in a line with its name. */
    private fun turn(name: String) = rule.onAllNodes(
        SemanticsMatcher.expectValue(SemanticsProperties.Selected, true) and hasText(name, substring = true),
    ).fetchSemanticsNodes().size == 1

    @Test
    fun theTwoSidesAreInALineAndTheSwapGivesTheChoicesToTheOther() {
        show()
        // our serve waits for its evaluation: no swap yet
        swap().assertIsNotEnabled()
        rule.onNode(hasText("+") and hasClickAction()).performClick()
        // the ball is on B's side: B receives
        rule.waitFor { turn("Arezzo") }
        swap().assertIsEnabled()
        swap().performClick()
        rule.waitFor { turn("Italia") }
        swap().performClick()
        rule.waitFor { turn("Arezzo") }
    }

    @Test
    fun theEventsOfTheServeRowSayWhoseTheyAre() {
        show()
        // our serve waits for its evaluation: the events of the rally in a
        // row with the serves, those of either side told apart by team
        val ofB = rule.onAllNodes(hasContentDescription(", Arezzo", substring = true) and hasClickAction())
        val ofA = rule.onAllNodes(hasContentDescription(", Italia", substring = true) and hasClickAction())
        assertTrue(ofB.fetchSemanticsNodes().isNotEmpty())
        assertTrue(ofA.fetchSemanticsNodes().isNotEmpty())
    }

    @Test
    fun theEventsOfBothSidesAreOfferedWithTheirTeam() {
        show()
        // the ball is still: a change of the lineup of either side, the
        // team named; B's goes in, the serve of A waiting for its evaluation
        rule.onNode(hasContentDescription(string(R.string.lineup)) and hasClickAction()).performClick()
        rule.waitForText("Arezzo", substring = true)
        val libero = core { c -> c.scouting(team, match).use { s -> s.otherEvents().first { it.sideB && it.value == EventTypeEnum.LR }.name } }
        rule.onNode(hasText(libero.replaceFirstChar { it.uppercase() }) and hasText("Arezzo") and hasClickAction()).performScrollTo().performClick()
        // B's libero is out: its touch is saved
        rule.waitFor({ "${rule.texts()} ${core { it.scouting(team, match) }.use { s -> s.state().events.size }}" }) { core { it.scouting(team, match) }.use { s -> s.state().events.isNotEmpty() } }
        assertEquals(true, core { it.scouting(team, match) }.use { s -> s.state().events.first().sideB })
        // the serve of A is still there to be evaluated
        assertTrue(rule.onAllNodes(hasText("#") and hasClickAction()).fetchSemanticsNodes().isNotEmpty())
    }

    @Test
    fun theStatisticsAreOfEitherSideOnItsChip() {
        serve(team, match, EvalEnum.PERFECT)
        show()
        rule.onNode(hasText(string(R.string.stats_swipe)) and hasClickAction()).performClick()
        // the chips of the two teams, A's chosen at first
        val chip = { name: String -> rule.onNode(hasText(name) and hasClickAction()) }
        rule.waitFor { rule.onAllNodes(hasText("Arezzo") and hasClickAction()).fetchSemanticsNodes().size == 1 }
        chip("Italia").assertIsSelected()
        chip("Arezzo").assertIsNotSelected().performClick()
        rule.waitFor { runCatching { chip("Arezzo").assertIsSelected() }.isSuccess }
        chip("Italia").assertIsNotSelected()
    }

    @Test
    fun theTwoSidesAreComparedOnTheirChip() {
        serve(team, match, EvalEnum.ERROR)
        show()
        rule.onNode(hasText(string(R.string.stats_swipe)) and hasClickAction()).performClick()
        val compare = { rule.onNode(hasText(string(R.string.compare)) and hasClickAction()) }
        rule.waitFor { runCatching { compare().assertIsNotSelected() }.isSuccess }
        compare().performClick()
        // the names of both teams over their columns, the rows between
        rule.waitFor { runCatching { compare().assertIsSelected() }.isSuccess }
        rule.waitFor { rule.onAllNodes(hasText(string(R.string.side_out))).fetchSemanticsNodes().isNotEmpty() }
        assertTrue(rule.onAllNodes(hasText("Arezzo") and !hasClickAction()).fetchSemanticsNodes().isNotEmpty())
        assertTrue(rule.onAllNodes(hasText("Italia") and !hasClickAction()).fetchSemanticsNodes().isNotEmpty())
        // the trend of both teams on one chart: their names in the legend,
        // the direct errors left out
        rule.onNode(hasText(string(R.string.stats_trend)) and hasClickAction()).performClick()
        rule.waitFor { rule.onAllNodes(hasText("╌╌ Arezzo"), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
        assertTrue(rule.onAllNodes(hasText("── Italia"), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty())
        assertTrue(rule.onAllNodes(hasText("│ " + string(R.string.trend_direct)), useUnmergedTree = true).fetchSemanticsNodes().isEmpty())
        // a fundamental in detail of both: the figures of each in one row
        rule.onNode(hasText(string(R.string.stats_detail)) and hasClickAction()).performClick()
        val efficiency = string(R.string.efficiency).replaceFirstChar { it.uppercase() }
        rule.waitFor { rule.onAllNodes(hasText(efficiency)).fetchSemanticsNodes().isNotEmpty() }
        assertTrue(rule.onAllNodes(hasText("Arezzo") and !hasClickAction()).fetchSemanticsNodes().isNotEmpty())
        assertTrue(rule.onAllNodes(hasText(string(R.string.rallies_per_point))).fetchSemanticsNodes().isNotEmpty())
        // a team's chip leaves the comparison
        rule.onNode(hasText("Italia") and hasClickAction()).performClick()
        rule.waitFor { runCatching { compare().assertIsNotSelected() }.isSuccess }
    }
}
