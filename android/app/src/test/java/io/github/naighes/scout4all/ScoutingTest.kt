package io.github.naighes.scout4all

import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.assertIsNotEnabled
import org.junit.Assert.assertTrue
import androidx.compose.ui.test.isSelected
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.semantics.SemanticsProperties
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

    private fun hasStateDescription(state: String) =
        SemanticsMatcher.expectValue(SemanticsProperties.StateDescription, state)

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

    /** Where our serve went, touched on the court while it waits for its
     * evaluation: a cell, then the one next to it along with it (two cells
     * of 1 and 9: on the line between them), saved with it; touched again,
     * taken back; never required. */
    @Test
    fun theServeZoneIsTouchedOnTheCourt() {
        show()
        val zone = { label: String -> hasContentDescription("${string(R.string.serve_zone)} $label") and hasClickAction() }
        val chosen = { rule.onAllNodes(hasStateDescription(string(R.string.zone_chosen))).fetchSemanticsNodes().size }
        rule.waitFor { rule.onAllNodes(zone("1B")).fetchSemanticsNodes().size == 1 }
        // chosen, taken back
        rule.onNode(zone("1B")).performClick()
        rule.waitFor { rule.onAllNodes(zone("1B") and hasStateDescription(string(R.string.zone_chosen))).fetchSemanticsNodes().size == 1 }
        rule.onNode(zone("1B")).performClick()
        rule.waitFor { chosen() == 0 }
        // 1B, then 9A under it: both chosen, told under the score; one
        // behind the other, no conflict zone
        rule.onNode(zone("1B")).performClick()
        rule.onNode(zone("9A")).performClick()
        rule.waitFor { chosen() == 2 }
        rule.waitForText("1B-9A", substring = true)
        assertTrue(rule.onAllNodes(hasText("(1/9)", substring = true)).fetchSemanticsNodes().isEmpty())
        touch("#")
        rule.waitFor { saved().events.size == 1 }
        assertEquals("1B-9A", saved().events[0].target)
        // the next serve, without one
        rule.waitFor { rule.onAllNodes(hasStateDescription(string(R.string.zone_chosen))).fetchSemanticsNodes().isEmpty() }
        touch("=")
        rule.waitFor { saved().events.size == 2 }
        // the last first
        assertEquals(listOf(null, "1B-9A"), saved().events.map { it.target })
        // their serve: the players again, no zones
        rule.waitFor { rule.onAllNodes(zone("8A")).fetchSemanticsNodes().isEmpty() }
    }

    /** Their serve: once the receiver is touched, our court with its
     * cells while the reception waits for its evaluation, the net at the
     * top; where we received saved with it. */
    @Test
    fun theReceptionZoneIsTouchedOnOurCourt() {
        show()
        val zone = { label: String -> hasContentDescription("${string(R.string.reception_zone)} $label") and hasClickAction() }
        touch("=")
        rule.waitFor { saved().events.size == 1 }
        assertTrue(rule.onAllNodes(zone("6A")).fetchSemanticsNodes().isEmpty())
        touch("18")
        rule.waitFor { rule.onAllNodes(zone("6A")).fetchSemanticsNodes().size == 1 }
        // ours: 4 at the net on the left, 1 at our end line on the right
        val top = { label: String -> rule.onNode(zone(label)).fetchSemanticsNode().positionInRoot }
        assertTrue(top("4C").y < top("1A").y && top("4C").x < top("1A").x)
        rule.onNode(zone("1D")).performClick()
        rule.onNode(zone("6A")).performClick()
        rule.waitForText("1D-6A (6/1)", substring = true)
        touch("+")
        rule.waitFor { saved().events.size == 2 }
        assertEquals("1D-6A (6/1)", saved().events[0].target)
    }

    @Test
    fun aServeErrorIsTheirPoint() {
        show()
        touch("=")
        rule.waitFor { score(saved()) == 0 to 1 }
    }

    /** The inverted P1, up in the bar in P1 in side out only, where it
     * makes a difference, goes with the events saved from then on: the set
     * resumed keeps it. */
    @Test
    fun theInvertedP1IsSavedWithTheEvents() {
        show()
        val chip = hasContentDescription(string(R.string.inverted_p1_description)) and hasClickAction()
        val chips = { rule.onAllNodes(chip).fetchSemanticsNodes().size }
        // we serve in P1: not there
        assertEquals(0, chips())
        // our serve error: they serve, P1 in side out
        touch("=")
        rule.waitFor { chips() == 1 }
        rule.onNode(chip).assertIsNotSelected().performClick()
        rule.waitFor { rule.onAllNodes(chip and isSelected()).fetchSemanticsNodes().size == 1 }
        touch(string(R.string.their_error))
        rule.waitFor { score(saved()) == 1 to 1 }
        assertTrue(saved().invertedP1)
        // we rotate and serve: gone again
        rule.waitFor { chips() == 0 }
    }

    /** The reception, up in the bar, only before their serve: 3, 4, 2,
     * written on their ace, then 3 again. */
    @Test
    fun theReceptionIsToldBeforeTheirServe() {
        show()
        val chip = { n: Int -> hasContentDescription(string(R.string.reception_description, n)) }
        rule.onNode(chip(3)).assertIsNotEnabled()
        // our serve error: they serve
        touch("=")
        rule.waitFor { rule.onAllNodes(chip(3) and isEnabled()).fetchSemanticsNodes().size == 1 }
        rule.onNode(chip(3)).performClick()
        rule.waitFor { saved().events.size == 1 && rule.onAllNodes(chip(4)).fetchSemanticsNodes().size == 1 }
        rule.onNode(chip(4)).assertIsSelected()
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
        // the setter can only attack: straight to the evaluation
        touch("8")
        touch("#")
        rule.waitFor { score(saved()) == 1 to 0 }
        // the last first: the attack of the setter, on the second touch
        val events = saved().events.map { listOf(it.name, it.symbol, it.player?.name) }
        assertEquals(listOf("attack", "#", "Alessia Orro"), events.first())
        assertEquals(3, events.size)
    }

    /** Our attack: where it went, touched on their court as our server
     * sees it (the net at the bottom, 1 at the top on the left). */
    @Test
    fun theAttackZoneIsTouchedOnTheirCourt() {
        show()
        val zone = { label: String -> hasContentDescription("${string(R.string.attack_zone)} $label") and hasClickAction() }
        touch("+")
        touch("18")
        touch("defense")
        touch("+")
        assertTrue(rule.onAllNodes(zone("5A")).fetchSemanticsNodes().isEmpty())
        // the setter can only attack: the evaluation and the court
        touch("8")
        rule.waitFor { rule.onAllNodes(zone("5A")).fetchSemanticsNodes().size == 1 }
        val at = { label: String -> rule.onNode(zone(label)).fetchSemanticsNode().positionInRoot }
        assertTrue(at("1A").y < at("4C").y && at("1A").x < at("4C").x)
        rule.onNode(zone("5A")).performClick()
        rule.waitForText("5A", substring = true)
        touch("#")
        rule.waitFor { score(saved()) == 1 to 0 }
        assertEquals("5A", saved().events[0].target)
    }

    /** Our dig: where we dug their attack, touched on our court (the net
     * at the top, 4 on the left). */
    @Test
    fun theDigZoneIsTouchedOnOurCourt() {
        show()
        val zone = { label: String -> hasContentDescription("${string(R.string.dig_zone)} $label") and hasClickAction() }
        touch("+")
        touch("18")
        touch("defense")
        rule.waitFor { rule.onAllNodes(zone("6B")).fetchSemanticsNodes().size == 1 }
        val at = { label: String -> rule.onNode(zone(label)).fetchSemanticsNode().positionInRoot }
        assertTrue(at("4C").y < at("1A").y && at("4C").x < at("1A").x)
        rule.onNode(zone("1C")).performClick()
        rule.onNode(zone("6B")).performClick()
        rule.waitForText("1C-6B (6/1)", substring = true)
        touch("+")
        rule.waitFor { saved().events.size == 2 }
        assertEquals("1C-6B (6/1)", saved().events[0].target)
    }

    /** Our block: where we blocked, only the zones at the net on our court
     * (4, 3, 2 from the left), whole: no cells. */
    @Test
    fun theBlockZoneIsTouchedAtTheNet() {
        show()
        val zone = { label: String -> hasContentDescription("${string(R.string.block_zone)} $label") and hasClickAction() }
        touch("+")
        // Paola Egonu, opposite, in the front row
        touch("18")
        touch("block")
        rule.waitFor { rule.onAllNodes(zone("3")).fetchSemanticsNodes().size == 1 }
        assertTrue(rule.onAllNodes(zone("3A")).fetchSemanticsNodes().isEmpty())
        assertTrue(rule.onAllNodes(zone("8")).fetchSemanticsNodes().isEmpty())
        val at = { label: String -> rule.onNode(zone(label)).fetchSemanticsNode().positionInRoot }
        assertTrue(at("4").x < at("3").x && at("3").x < at("2").x)
        rule.onNode(zone("2")).performClick()
        touch("+")
        rule.waitFor { saved().events.size == 2 }
        assertEquals("2", saved().events[0].target)
    }

    /** From the changes of the lineup, the libero for a player touched on
     * court: saved with the player; in their place after a rally. */
    @Test
    fun theLiberoForAPlayerOfTheBackRow() {
        show()
        rule.onNode(hasContentDescription(string(R.string.lineup)) and hasClickAction()).performClick()
        touch(string(R.string.libero_for_short))
        // Ekaterina Antropova, outside hitter 2, in position 5
        touch("9")
        rule.waitFor { saved().events.size == 1 }
        val first = saved().events.first()
        assertEquals(EventTypeEnum.LF, first.event)
        assertEquals("Ekaterina Antropova", first.player?.name)
        // out of the middle blocker at once, in for her after our ace
        assertTrue(saved().court.none { it.libero })
        touch("#")
        rule.waitFor { score(saved()) == 1 to 0 }
        val court = saved().court
        assertTrue(court.any { it.libero })
        assertTrue(court.none { it.name == "Ekaterina Antropova" })
        assertTrue(court.any { it.name == "Anna Danesi" })
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

