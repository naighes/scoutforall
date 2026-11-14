package io.github.naighes.scout4all

import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import io.github.naighes.scout4all.core.MatchForm
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** A match between two teams, scouted by A: listed by B too, with only its
 * result as B sees it (step 21). */
@RunWith(RobolectricTestRunner::class)
class SeenByBTest {
    @get:Rule val rule = createComposeRule()

    private val italia = saveNationalTeam()
    private val serbia = saveTeam("Serbia")

    /** Italia–Serbia, Italia at home, set 1 won 25–0 by Italia. */
    private val match: String = core { c ->
        c.saveMatch(italia, null, MatchForm("Serbia", "2026-10-25T18:00:00+02:00", true, "6", opponentTeam = serbia))
    }.also { m ->
        startNextSet(italia, m)
        serve(italia, m, *Array(25) { io.github.naighes.scout4all.core.EvalEnum.PERFECT })
    }

    @Test
    fun theMatchIsListedByB() {
        var opened: String? = null
        rule.setContent {
            Scout4AllTheme {
                TeamScreen(serbia, onBack = {}, onEdit = {}, onNewPlayer = {}, onPlayer = {}, onNewMatch = {}, onMatch = { opened = it })
            }
        }
        rule.waitForText(string(R.string.no_players))
        rule.onNode(hasText(string(R.string.matches)) and hasClickAction()).performClick()
        rule.waitForText(string(R.string.scouted_by_opponent))
        rule.onNodeWithText("Italia").assertExists()
        rule.onNodeWithText("Italia").performClick()
        assertEquals(match, opened)
    }

    @Test
    fun theMatchOfBShowsItsResultAndOpensAsA() {
        var openedAs: String? = null
        rule.setContent {
            Scout4AllTheme {
                MatchScreen(
                    serbia, match, onBack = {}, onEdit = {}, onStartSet = {}, onResumeSet = {}, onSet = {}, onStats = {},
                    onOpenAs = { openedAs = it },
                )
            }
        }
        rule.waitForText(string(R.string.scouted_by_text, "Italia", "Serbia"))
        // the set as B sees it, nothing to change or scout
        rule.onNodeWithText(string(R.string.score_to, 0, 25)).assertExists()
        rule.onAllNodesWithContentDescription(string(R.string.edit_match)).assertCountEquals(0)
        rule.onAllNodesWithContentDescription(string(R.string.delete_match)).assertCountEquals(0)
        rule.onNode(hasText(string(R.string.match_stats))).assertDoesNotExist()
        rule.onNode(hasText(string(R.string.open_as_team, "Italia")) and hasClickAction()).performClick()
        assertEquals(italia, openedAs)
    }
}
