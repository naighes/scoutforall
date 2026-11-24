package io.github.naighes.scout4all

import androidx.compose.foundation.layout.Column
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.performSemanticsAction
import io.github.naighes.scout4all.core.DetailFilter
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.MatchSide
import io.github.naighes.scout4all.core.StatsScope
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The photo of the player in the table of the players of the detail. */
@RunWith(RobolectricTestRunner::class)
class StatsPhotoTest {
    @get:Rule val rule = createComposeRule()

    private fun fixture(name: String) = javaClass.classLoader!!.getResourceAsStream(name)!!.use { it.readBytes() }

    @Test
    fun thePhotoOfAPlayerIsShownInTheTableOfThePlayers() {
        val team = saveNationalTeam()
        val match = startFirstSet(team)
        serve(team, match, EvalEnum.PERFECT)
        val photo = core { it.cropFace(fixture("solvay_one_face.jpg")) }
        core { c -> c.team(team).players.forEach { c.setPersonPhoto(it.id, photo) } }
        val stats = core { it.matchStats(team, match, MatchSide.A, StatsScope.Match, DetailFilter(null)) }
        rule.setContent {
            Scout4AllTheme {
                Column {
                    StatsPages(stats, onScope = {}, loadDetail = { scope, event, filter ->
                        app.core { it.matchDetail(team, match, MatchSide.A, scope, event, filter) }
                    })
                }
            }
        }
        rule.onNode(hasText(string(R.string.stats_detail)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.players_title))
        rule.waitFor { rule.onAllNodesWithTag(SHOWN_IMAGE, useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty() }
    }
}
