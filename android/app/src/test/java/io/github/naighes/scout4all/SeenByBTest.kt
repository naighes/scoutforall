package io.github.naighes.scout4all

import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsProperties
import io.github.naighes.scout4all.core.DvwChoice
import io.github.naighes.scout4all.core.DvwOpponent
import io.github.naighes.scout4all.core.DvwSide
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.MatchSide
import io.github.naighes.scout4all.core.TeamListing
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

    /** "Home"–"Visiting" of the TUI fixture, imported into "Home" on both
     * sides, "Visiting" a new team: the match of A and the team B. */
    private fun bothSides(): Triple<String, String, String> {
        val home = saveTeam("Home")
        val bytes = javaClass.classLoader!!.getResourceAsStream("three_sets.dvw")!!.use { it.readBytes() }
        val choice = DvwChoice(
            side = DvwSide.HOME,
            day = "2015-01-25",
            maxSubstitutions = 6u,
            teamId = home,
            classification = null,
            gender = null,
            country = null,
            opponent = DvwOpponent.NewTeam,
        )
        val m = core { it.importDvw(bytes, choice) }.matchId
        val b = core { it.teams() }.firstNotNullOf { t -> (t as? TeamListing.Team)?.takeIf { it.name == "Visiting" }?.id }
        return Triple(home, m, b)
    }

    @Test
    fun theMatchOfBOnBothSidesOpensItsStatistics() {
        val (home, m, b) = bothSides()
        var statsOf: String? = null
        rule.setContent {
            Scout4AllTheme {
                MatchScreen(b, m, onBack = {}, onEdit = {}, onStartSet = {}, onResumeSet = {}, onSet = {}, onStats = {}, onStatsOfB = { statsOf = it })
            }
        }
        rule.waitForText(string(R.string.scouted_both_text, "Home", "Visiting"))
        rule.onNode(hasText(string(R.string.match_stats)) and hasClickAction()).performClick()
        assertEquals(home, statsOf)
    }

    @Test
    fun theStatisticsOfBAreOfItsSide() {
        val (home, m, _) = bothSides()
        rule.setContent { Scout4AllTheme { MatchStatsScreen(home, m, null, MatchSide.B, onBack = {}) } }
        rule.waitForText(string(R.string.stats_summary))
        rule.onNode(hasText(string(R.string.stats_trend)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForIdle()
        // B's color first in the legend of the points
        val legend = rule.onAllNodesWithText("■ ", substring = true, useUnmergedTree = true).fetchSemanticsNodes()
            .map { it.config[SemanticsProperties.Text].joinToString() }
        assertEquals(listOf("■ Visiting", "■ Home"), legend)
    }

    /** The actions of B to watch, like A's (there's no video here: said). */
    @Test
    fun theActionsOfBCanBeWatched() {
        val (home, m, _) = bothSides()
        var watched: VideoRequest.Clips? = null
        rule.setContent { Scout4AllTheme { MatchStatsScreen(home, m, null, MatchSide.B, onBack = {}) { watched = it } } }
        rule.waitForText(string(R.string.stats_summary))
        rule.onNode(hasText(string(R.string.stats_detail)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.clips_no_video))
        assertEquals(null, watched)
    }
}
