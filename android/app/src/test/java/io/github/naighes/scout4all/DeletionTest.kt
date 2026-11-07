package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import io.github.naighes.scout4all.core.MatchListing
import io.github.naighes.scout4all.core.TeamListing
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.io.File

/** A team and a match deleted from their pages, an unreadable match from
 * the list: for good, once confirmed, as loudly asked. */
@RunWith(RobolectricTestRunner::class)
class DeletionTest {
    @get:Rule val rule = createComposeRule()

    private var deleted = 0

    private fun teams() = core { it.teams() }.filterIsInstance<TeamListing.Team>().map { it.name }

    @Test
    fun aTeamDeletedOnceConfirmed() {
        val team = saveNationalTeam()
        saveTeam("Serbia")
        startFirstSet(team)
        rule.setContent {
            Scout4AllTheme {
                TeamScreen(
                    team, onBack = {}, onEdit = {}, onNewPlayer = {}, onPlayer = {}, onNewMatch = {}, onMatch = {},
                    onDeleted = { deleted++ },
                )
            }
        }
        rule.waitForText("Alessia Orro")
        rule.onNodeWithContentDescription(string(R.string.delete_team)).performClick()
        rule.onNodeWithText(string(R.string.permanent_deletion)).assertExists()
        rule.onNodeWithText(string(R.string.irreversible)).assertExists()
        rule.onNodeWithText(string(R.string.delete_team_text, "Italia")).assertExists()
        rule.onNodeWithText(string(R.string.cancel)).performClick()
        assertEquals(listOf("Italia", "Serbia"), teams())
        rule.onNodeWithContentDescription(string(R.string.delete_team)).performClick()
        rule.onNodeWithText(string(R.string.delete_forever)).performClick()
        rule.waitFor { deleted == 1 }
        assertEquals(listOf("Serbia"), teams())
    }

    @Test
    fun aMatchDeletedOnceConfirmed() {
        val team = saveNationalTeam()
        val match = startFirstSet(team)
        rule.setContent {
            Scout4AllTheme {
                MatchScreen(
                    team, match, onBack = {}, onEdit = {}, onStartSet = {}, onResumeSet = {}, onSet = {}, onStats = {},
                    onDeleted = { deleted++ },
                )
            }
        }
        rule.waitForText(string(R.string.resume_set_n, 1))
        rule.onNodeWithContentDescription(string(R.string.delete_match)).performClick()
        rule.onNodeWithText(string(R.string.delete_forever)).performClick()
        rule.waitFor { deleted == 1 }
        assertEquals(emptyList<MatchListing>(), core { it.matches(team) })
        // the team and its players stay
        assertEquals(7, core { it.team(team) }.players.size)
    }

    @Test
    fun anUnreadableMatchDeletedFromTheList() {
        val team = saveNationalTeam()
        val folder = File(app.filesDir, "scout4all").resolve(team).resolve("not-a-match").apply { mkdirs() }
        rule.setContent {
            Scout4AllTheme {
                TeamScreen(team, onBack = {}, onEdit = {}, onNewPlayer = {}, onPlayer = {}, onNewMatch = {}, onMatch = {})
            }
        }
        rule.waitForText("Alessia Orro")
        rule.onNode(hasText(string(R.string.matches)) and hasClickAction()).performClick()
        rule.waitForText(string(R.string.unreadable_match))
        rule.onNodeWithText(string(R.string.unreadable_match)).performClick()
        rule.onNodeWithText(string(R.string.delete_forever)).performClick()
        rule.waitForText(string(R.string.no_matches))
        assertTrue(!folder.exists())
    }
}
