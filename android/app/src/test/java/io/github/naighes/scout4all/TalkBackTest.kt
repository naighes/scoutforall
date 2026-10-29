package io.github.naighes.scout4all

import androidx.compose.runtime.Composable
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.performClick
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isHeading
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.semantics.SemanticsActions
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.MatchForm
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** What TalkBack reads: the symbols as words, a score as a whole, the
 * badges of a player after the name, the titles to move between. */
@RunWith(RobolectricTestRunner::class)
class TalkBackTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()

    private fun show(shown: String, content: @Composable () -> Unit) {
        rule.setContent { Scout4AllTheme { content() } }
        rule.waitForText(shown, substring = true)
    }

    /** What TalkBack reads of each thing it stops on: the description, or
     * else the texts. */
    private fun spoken(): List<String> =
        rule.onAllNodes(SemanticsMatcher("any") { true }).fetchSemanticsNodes().mapNotNull { node ->
            val description = node.config.getOrNull(SemanticsProperties.ContentDescription)?.joinToString(", ")
            description ?: node.config.getOrNull(SemanticsProperties.Text)?.joinToString(", ")?.takeIf { it.isNotBlank() }
        }

    private fun assertSpoken(text: String) = spoken().let { assertTrue("no \"$text\" in $it", text in it) }

    private fun assertTitle(text: String) =
        rule.waitFor({ "no title \"$text\" in ${rule.texts()}" }) {
            rule.onAllNodes(hasText(text) and isHeading()).fetchSemanticsNodes().isNotEmpty()
        }

    @Test
    fun theScoutingReadsTheScoreAndThePlayersInWords() {
        val match = startFirstSet(team)
        show("Cortona") { ScoutingScreen(team, match, onBack = {}) }
        assertSpoken("Italia 0, Cortona 0, " + string(R.string.serves, "Italia"))
        // the server in the serve zone: the setter, with the ball
        assertSpoken("8, Alessia Orro, ${string(R.string.setter)}, ${string(R.string.serving)}")
        assertSpoken("18, Paola Egonu")
    }

    /** The score in a live region: TalkBack says it when it changes. */
    private fun announced(): List<String> =
        rule.onAllNodes(SemanticsMatcher.expectValue(SemanticsProperties.LiveRegion, LiveRegionMode.Polite))
            .fetchSemanticsNodes().mapNotNull { it.config.getOrNull(SemanticsProperties.Text)?.joinToString(", ") }

    @Test
    fun aPointIsAnnounced() {
        val match = startFirstSet(team)
        show("Cortona") { ScoutingScreen(team, match, onBack = {}) }
        assertEquals(listOf("Italia 0, Cortona 0, " + string(R.string.serves, "Italia")), announced())
        rule.onNode(hasContentDescription("ace") and hasClickAction()).performClick()
        rule.waitFor({ "${announced()}" }) { announced() == listOf("Italia 1, Cortona 0, " + string(R.string.serves, "Italia")) }
    }

    @Test
    fun theEndOfTheSetSaysTheScore() {
        val match = startFirstSet(team)
        serve(team, match, *Array(24) { EvalEnum.PERFECT })
        show("Cortona") { ScoutingScreen(team, match, onBack = {}) }
        rule.onNode(hasContentDescription("ace") and hasClickAction()).performClick()
        rule.waitForText(string(R.string.set_won, 1))
        assertSpoken(string(R.string.score_to, 25, 0))
    }

    @Test
    fun theLastTouchIsReadAfterTheName() {
        val match = startFirstSet(team)
        serve(team, match, EvalEnum.POSITIVE)
        show("Cortona") { ScoutingScreen(team, match, onBack = {}) }
        rule.waitFor({ "${spoken()}" }) { spoken().any { it.startsWith("8, Alessia Orro, ${string(R.string.setter)}, ") } }
        val orro = spoken().first { it.startsWith("8, Alessia Orro") }
        assertTrue(orro, orro.contains(string(R.string.last_touch, "positive")))
    }

    @Test
    fun anEmptyPositionSaysSo() {
        val match = core { it.saveMatch(team, null, MatchForm("Cortona", "2026-10-25T18:00:00+02:00", true, "6")) }
        show(string(R.string.start_set)) { SetStartScreen(team, match, onBack = {}, onStarted = {}) }
        assertSpoken("${string(R.string.position_spoken, 1)}, ${string(R.string.no_player)}")
        assertTitle(string(R.string.start_set_n, 1))
        assertTitle(string(R.string.serving_team))
    }

    @Test
    fun theEventsOfTheSetSayTheEvaluation() {
        val match = startFirstSet(team)
        serve(team, match, EvalEnum.PERFECT)
        show(string(R.string.statistics)) { SetScreen(team, match, 1, onBack = {}, onStats = {}) }
        rule.waitForText(string(R.string.score_to, 1, 0))
        assertSpoken("${string(R.string.event_n, 1)}, ace, Serve · F, 8 Alessia Orro, ${string(R.string.score_to, 1, 0)}")
        assertTitle(string(R.string.set_n, 1))
    }

    @Test
    fun theMatchReadsItsSets() {
        val match = startFirstSet(team)
        serve(team, match, EvalEnum.PERFECT)
        show(string(R.string.match_stats)) {
            MatchScreen(team, match, onBack = {}, onEdit = {}, onStartSet = {}, onResumeSet = {}, onSet = {}, onStats = {})
        }
        assertSpoken(string(R.string.sets_score, 0, 0))
        assertSpoken("${string(R.string.set_n, 1)}, ${string(R.string.in_progress)}, ${string(R.string.score_to, 1, 0)}")
        assertTitle("Cortona")
    }

    @Test
    fun aMatchOfTheTeamIsReadWhole() {
        startFirstSet(team)
        show("Ekaterina") {
            TeamScreen(team, onBack = {}, onEdit = {}, onNewPlayer = {}, onPlayer = {}, onNewMatch = {}, onMatch = {})
        }
        rule.onNode(hasText(string(R.string.matches)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitFor({ "${spoken()}" }) { spoken().any { it.startsWith("${string(R.string.home)}, Cortona, ") } }
        val row = spoken().first { it.startsWith("${string(R.string.home)}, Cortona") }
        assertTrue(row, row.endsWith("${string(R.string.in_progress)}, ${string(R.string.sets_score, 0, 0)}"))
        assertTitle("Italia")
    }

    @Test
    fun theStatisticsReadEachTileAndChartAsOne() {
        val match = startFirstSet(team)
        serve(team, match, EvalEnum.PERFECT, EvalEnum.POSITIVE)
        show(string(R.string.stats_summary)) { MatchStatsScreen(team, match, null, onBack = {}) }
        assertSpoken("${string(R.string.break_points)}, 100%, ${string(R.string.rallies_detail, 1, 1)}")
        assertSpoken("Serve, 100%, ${string(R.string.touches_spoken, 2, 1, 0)}")
        assertTitle(string(R.string.rotations))
        // we served: no possession of ours
        assertTitle(string(R.string.side_out_by_possession))
        assertSpoken("1st, –")
        rule.onNode(hasText(string(R.string.stats_trend)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitFor({ "${spoken()}" }) { spoken().any { it.startsWith("Serve, ") && it.contains(string(R.string.set_n, 1) + " ") } }
    }

    @Test
    fun theTitlesOfTheForms() {
        show(string(R.string.classification)) { TeamFormScreen(null, onBack = {}, onSaved = {}) }
        assertTitle(string(R.string.new_team))
        assertTitle(string(R.string.gender))
        assertTitle(string(R.string.classification))
    }
}
