package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.MatchListing
import io.github.naighes.scout4all.core.TeamListing
import org.junit.Assert.assertEquals
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
        rule.onNodeWithText(string(R.string.delete_team_text, "Italia", 1)).assertExists()
        rule.onNodeWithText(string(R.string.cancel)).performClick()
        assertEquals(listOf("Italia", "Serbia"), teams())
        rule.onNodeWithContentDescription(string(R.string.delete_team)).performClick()
        rule.onNodeWithText(string(R.string.delete_forever)).performClick()
        rule.waitFor { deleted == 1 }
        assertEquals(listOf("Serbia"), teams())
    }

    /** The matches against another team stay to it, as the dialog says:
     * the other team keeps them. */
    @Test
    fun theMatchesAgainstAnotherTeamStayToIt() {
        val team = saveNationalTeam()
        val serbia = saveTeam("Serbia")
        startFirstSet(team)
        core { it.saveMatch(team, null, MatchForm("Serbia", "2026-10-26T18:00:00+02:00", true, "6", opponentTeam = serbia)) }
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
        val text = string(R.string.delete_team_text, "Italia", 1) + " " + string(R.string.delete_team_kept, 1, "Italia")
        rule.waitForText(text)
        rule.onNodeWithText(string(R.string.delete_forever)).performClick()
        rule.waitFor { deleted == 1 }
        assertEquals(listOf("Serbia"), teams())
        assertEquals(1, core { it.matches(serbia) }.size)
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
        // a team of a version before the database, its match with a set
        // this version doesn't replay (an ace after its end): moved into
        // the database at the start, unreadable there
        val team = "15695ad8-cfbc-418f-b7b1-0d3d9860e9be"
        val legacy = javaClass.classLoader!!.getResource("legacy/v0.18.0/data/$team")!!
        val folder = File(app.filesDir, "scout4all/$team")
        File(legacy.toURI()).copyRecursively(folder)
        folder.resolve("2025-10-25_visiting/set_1.csv")
            .appendText("2025-10-24T22:01:00Z,S,b5252927-15b5-4df3-8b6b-a6138dbd08af,#,,F,\n")
        rule.setContent {
            Scout4AllTheme {
                TeamScreen(team, onBack = {}, onEdit = {}, onNewPlayer = {}, onPlayer = {}, onNewMatch = {}, onMatch = {})
            }
        }
        rule.waitForText("Name1 Surname1")
        rule.onNode(hasText(string(R.string.matches)) and hasClickAction()).performClick()
        rule.waitForText(string(R.string.unreadable_match))
        rule.onNodeWithText(string(R.string.unreadable_match)).performClick()
        rule.onNodeWithText(string(R.string.delete_forever)).performClick()
        rule.waitForText(string(R.string.no_matches))
        assertEquals(emptyList<MatchListing>(), core { it.matches(team) })
    }
}
