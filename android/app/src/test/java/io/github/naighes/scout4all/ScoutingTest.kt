package io.github.naighes.scout4all

import org.junit.Assert.assertTrue
import androidx.compose.ui.test.isSelected
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.performClick
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.EventTypeEnum
import io.github.naighes.scout4all.core.Scouting
import io.github.naighes.scout4all.core.ScoutingState
import io.github.naighes.scout4all.core.TeamSideEnum
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Scouting a set, as the scout touches it; the core saves each event. */
@RunWith(RobolectricTestRunner::class)
class ScoutingTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()
    private val match = startFirstSet(team)
    private var back = 0

    private fun show() {
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = { back++ }) } }
        rule.waitForText("Cortona", substring = true)
    }

    /** What the core saved, read again as when the set is resumed. */
    private fun saved(): ScoutingState = core { it.scouting(team, match) }.state()

    private fun score(state: ScoutingState) = state.scoreUs.toInt() to state.scoreThem.toInt()

    private fun touch(text: String) {
        val touchable = hasText(text) and hasClickAction() and isEnabled()
        rule.waitFor { rule.onAllNodes(touchable).fetchSemanticsNodes().size == 1 }
        rule.onNode(touchable).performClick()
    }

    private fun undo() {
        rule.onNode(hasContentDescription(string(R.string.undo)) and hasClickAction()).performClick()
    }

    @Test
    fun theChoicesMeasuredForTheHeightAreNotThere() {
        show()
        // not even for TalkBack
        assertEquals(1, rule.onAllNodes(hasText("#") and hasClickAction()).fetchSemanticsNodes().size)
    }

    @Test
    fun anAceIsOurPoint() {
        show()
        touch("#")
        rule.waitFor { score(saved()) == 1 to 0 }
        rule.waitForText("serve #", substring = true)
    }

    @Test
    fun aServeErrorIsTheirPoint() {
        show()
        touch("=")
        rule.waitFor { score(saved()) == 0 to 1 }
    }

    /** The inverted P1, up in the bar, goes with the events saved from
     * then on: the set resumed keeps it. */
    @Test
    fun theInvertedP1IsSavedWithTheEvents() {
        show()
        val chip = hasContentDescription(string(R.string.inverted_p1_description)) and hasClickAction()
        rule.onNode(chip).assertIsNotSelected().performClick()
        rule.waitFor { rule.onAllNodes(chip and isSelected()).fetchSemanticsNodes().size == 1 }
        touch("#")
        rule.waitFor { score(saved()) == 1 to 0 }
        assertTrue(saved().invertedP1)
    }

    @Test
    fun undoTakesBackTheLastEvent() {
        show()
        touch("#")
        rule.waitFor { score(saved()) == 1 to 0 }
        undo()
        rule.waitFor { score(saved()) == 0 to 0 }
        rule.waitFor { saved().events.isEmpty() }
    }

    @Test
    fun aRallyThroughThePlayers() {
        show()
        touch("+")
        touch("18")
        touch("defense")
        touch("+")
        touch("8")
        touch("attack")
        touch("#")
        rule.waitFor { score(saved()) == 1 to 0 }
        // the last first: the attack of the setter, on the second touch
        val events = saved().events.map { listOf(it.name, it.symbol, it.player?.name) }
        assertEquals(listOf("attack", "#", "Alessia Orro"), events.first())
        assertEquals(3, events.size)
    }

    @Test
    fun theSetOverAsksToGoBackOrUndo() {
        show()
        repeat(25) { touch("#") }
        rule.waitForText(string(R.string.set_won, 1))
        rule.waitForText(string(R.string.score_to, 25, 0))
        // saved: no set in progress, the next one to start
        core { it.setStart(team, match) }
        // a mistake: the last point goes back, the set goes on
        rule.onNode(hasText(string(R.string.undo)) and hasClickAction()).performClick()
        rule.waitFor { score(saved()) == 24 to 0 }
        touch("#")
        rule.waitForText(string(R.string.set_won, 1))
        rule.onNode(hasText(string(R.string.ok)) and hasClickAction()).performClick()
        rule.waitFor { back == 1 }
    }

    /** The two sets before scouted with the core, the last one here but its
     * last point, touched with `last`: `point` scores each. */
    private fun lastSet(point: (Scouting) -> Unit, last: () -> Unit) {
        repeat(3) { set ->
            core { c -> c.scouting(team, match).use { s -> repeat(if (set < 2) 25 else 24) { point(s) } } }
            if (set < 2) startNextSet(team, match)
        }
        show()
        last()
    }

    private val ourAce = { s: Scouting -> s.chooseEval(EvalEnum.PERFECT) }
    // our serve wrong, then theirs a point
    private val theirPoint = { s: Scouting ->
        when (s.state().servingTeam) {
            TeamSideEnum.US -> s.chooseEval(EvalEnum.ERROR)
            TeamSideEnum.THEM -> s.chooseEvent(EventTypeEnum.OS)
        }
    }

    @Test
    fun theSetThatWinsTheMatchSaysSo() {
        lastSet({ ourAce(it) }) { touch("#") }
        rule.waitForText(string(R.string.match_won))
        rule.waitForText(string(R.string.sets_score, 3, 0))
        rule.waitForText(List(3) { string(R.string.score_to, 25, 0) }.joinToString(", "))
        rule.waitForText(string(R.string.match_won_cheer))
        rule.shot("match-won")
        rule.onNode(hasText(string(R.string.ok)) and hasClickAction()).performClick()
        rule.waitFor { back == 1 }
    }

    @Test
    fun theSetThatLosesTheMatchSaysSoAndCanBeUndone() {
        lastSet({ theirPoint(it) }) { touch(string(R.string.their_point)) }
        rule.waitForText(string(R.string.match_lost))
        rule.waitForText(string(R.string.sets_score, 0, 3))
        rule.waitForText(string(R.string.match_lost_cheer))
        rule.shot("match-lost")
        assertEquals(0, rule.onAllNodesWithText(string(R.string.match_won_cheer)).fetchSemanticsNodes().size)
        // a mistake: the last point goes back, the match goes on
        rule.onNode(hasText(string(R.string.undo)) and hasClickAction()).performClick()
        rule.waitFor { score(saved()) == 0 to 24 }
    }
}

