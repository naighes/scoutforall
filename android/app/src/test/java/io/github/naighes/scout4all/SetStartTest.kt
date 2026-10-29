package io.github.naighes.scout4all

import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsNotSelected
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasScrollAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performClick
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import io.github.naighes.scout4all.core.TeamSideEnum
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class SetStartTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()
    private val match = core { it.saveMatch(team, null, MatchForm("Cortona", "2026-10-25T18:00:00+02:00", true, "6")) }
    private var started = 0

    private fun show(match: String = this.match, shown: String = string(R.string.start_set)) {
        rule.setContent { Scout4AllTheme { SetStartScreen(team, match, onBack = {}, onStarted = { started++ }) } }
        rule.waitForText(shown)
    }

    private fun start() = rule.onNode(hasText(string(R.string.start_set)) and hasClickAction())

    /** A player in the list of the choice open, not on the court. */
    private fun pick(name: String) {
        rule.waitFor { rule.onAllNodes(hasText(name) and hasAnyAncestor(hasScrollAction())).fetchSemanticsNodes().size == 1 }
        rule.onNode(hasText(name) and hasAnyAncestor(hasScrollAction())).performClick()
    }

    @Test
    fun theLineupGoesInAtAGlanceThenTheSetStarts() {
        show()
        start().assertIsNotEnabled()
        rule.onNode(hasText("Italia") and hasClickAction()).performClick()
        // position 1, then each empty one opens by itself
        rule.onNode(hasText(string(R.string.position_spoken, 1)) and hasClickAction()).performClick()
        nationalTeam.take(6).forEach { (name, _, _) -> pick(name) }
        pick("Alessia Orro")
        pick("Monica De Gennaro")
        rule.waitFor { runCatching { start().assertIsEnabled() }.isSuccess }
        start().performClick()
        rule.waitFor { started == 1 }
        val state = core { it.scouting(team, match) }.state()
        assertEquals(TeamSideEnum.US, state.servingTeam)
        // the libero in for the middle blocker in the back row
        val court = nationalTeam.take(5).map { it.first } + "Monica De Gennaro"
        assertEquals(court.toSet(), state.court.map { it.name }.toSet())
    }

    @Test
    fun noSetStartsWithoutTheLineup() {
        show()
        rule.onNode(hasText("Italia") and hasClickAction()).performClick()
        rule.onNode(hasText(string(R.string.position_spoken, 1)) and hasClickAction()).performClick()
        pick("Alessia Orro")
        // the next position opened by itself: closed
        rule.onNode(hasContentDescription(string(R.string.close))).performClick()
        rule.waitForIdle()
        start().assertIsNotEnabled()
        assertEquals(0, started)
    }

    /** A match with its first set won with aces; its id. */
    private fun firstSetWon(): String {
        val match = startFirstSet(team, opponent = "Arezzo")
        serve(team, match, *Array(25) { EvalEnum.PERFECT })
        return match
    }

    @Test
    fun theSecondSetStartsAsTheFirstDid() {
        val match = firstSetWon()
        show(match)
        rule.waitFor { runCatching { start().assertIsEnabled() }.isSuccess }
        start().performClick()
        rule.waitFor { started == 1 }
        val state = core { it.scouting(team, match) }.state()
        // who received first serves
        assertEquals(TeamSideEnum.THEM, state.servingTeam)
        // the libero in for the middle blocker in the back row
        val court = nationalTeam.take(5).map { it.first } + "Monica De Gennaro"
        assertEquals(court.toSet(), state.court.map { it.name }.toSet())
    }

    /** On the score sheet of the match: removed from the team since the
     * last set, a player is still there. */
    @Test
    fun aPlayerRemovedFromTheTeamStaysInTheMatch() {
        val match = firstSetWon()
        val orro = core { it.team(team) }.players.first { it.name == "Alessia Orro" }.id
        core { it.removePlayer(team, orro) }
        show(match)
        rule.waitFor { runCatching { start().assertIsEnabled() }.isSuccess }
    }

    /** A player added to the team plays in the match once on its score
     * sheet, in place of one who didn't play. */
    @Test
    fun aPlayerAddedToTheTeamPlaysOnceOnTheScoreSheet() {
        val match = firstSetWon()
        val cambi = core { it.savePlayer(team, null, PlayerForm("Carlotta Cambi", RoleEnum.SETTER, "10")) }
        val form = core { it.matchForm(team, match) }
        core { it.saveMatch(team, match, form.copy(roster = form.roster.orEmpty() + cambi)) }
        show(match)
        rule.onNode(hasText(string(R.string.position_spoken, 1)) and hasClickAction()).performClick()
        pick("Carlotta Cambi")
    }

    /** Who serves by the names of the teams; the ball by the one chosen. */
    @Test
    fun theTeamsServingAreNamed() {
        show()
        rule.onNode(hasText("Cortona") and hasClickAction()).performClick()
        rule.onNode(hasText("Cortona") and hasClickAction()).assertIsSelected()
        rule.onNode(hasText("Italia") and hasClickAction()).assertIsNotSelected()
    }
}
