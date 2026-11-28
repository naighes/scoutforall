package io.github.naighes.scout4all

import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The bar of a team: the analysis and the training plan as icons, the
 * rare actions (export, edit, new season, delete) in the menu, nothing of them left
 * among the matches. */
@RunWith(RobolectricTestRunner::class)
class TeamBarTest {
    @get:Rule val rule = createComposeRule()

    private var analysis = 0
    private var training = 0
    private var edited = 0
    private var seasons = 0

    private fun show() {
        val team = saveNationalTeam()
        startFirstSet(team)
        show(team)
        rule.waitForText("Ekaterina Antropova")
    }

    private fun show(team: String) {
        rule.setContent {
            Scout4AllTheme {
                TeamScreen(
                    team, onBack = {}, onEdit = { edited++ }, onNewPlayer = {}, onPlayer = {}, onNewMatch = {}, onMatch = {},
                    onAnalysis = { analysis++ }, onTraining = { training++ },
                    onNewSeason = { seasons++ },
                )
            }
        }
    }

    /** The new season from the menu, not a button after the players. */
    @Test
    fun theNewSeasonIsInTheMenu() {
        show()
        rule.onAllNodesWithText(string(R.string.new_season)).assertCountEquals(0)
        rule.onNodeWithContentDescription(string(R.string.more_actions)).performClick()
        rule.onNodeWithText(string(R.string.new_season)).performClick()
        assertEquals(1, seasons)
    }

    /** The menu hides the team from the home, then offers to show it. */
    @Test
    fun theMenuHidesTheTeamFromTheHome() {
        val team = saveTeam("Cortona")
        show(team)
        rule.waitForText(string(R.string.no_players))
        val menu = { rule.onNodeWithContentDescription(string(R.string.more_actions)).performClick() }
        menu()
        rule.onNodeWithText(string(R.string.hide_from_home)).performClick()
        rule.waitFor { core { it.teamHidden(team) } }
        menu()
        rule.onNodeWithText(string(R.string.show_in_home)).performClick()
        rule.waitFor { !core { it.teamHidden(team) } }
    }

    /** Without a logo its initials head the team, as in the lists. */
    @Test
    fun theInitialsHeadATeamWithoutALogo() {
        show(saveTeam("Italia U19"))
        rule.waitFor {
            rule.onAllNodesWithTag(initialsTag("IU"), useUnmergedTree = true).fetchSemanticsNodes().isNotEmpty()
        }
    }

    @Test
    fun aTeamWithoutPlayersHasTheNewSeasonToo() {
        show(saveTeam("Italia U19"))
        rule.waitForText(string(R.string.no_players))
        rule.onAllNodesWithText(string(R.string.new_season)).assertCountEquals(0)
        rule.onNodeWithContentDescription(string(R.string.more_actions)).performClick()
        rule.onNodeWithText(string(R.string.new_season)).performClick()
        assertEquals(1, seasons)
    }

    @Test
    fun theAnalysisAndTheTrainingPlanAreInTheBar() {
        show()
        rule.onNode(hasText(string(R.string.matches)) and hasClickAction()).performClick()
        rule.waitForText("Cortona")
        // not buttons among the matches any more: icons, found by their description
        rule.onAllNodesWithText(string(R.string.analysis)).assertCountEquals(0)
        rule.onAllNodesWithText(string(R.string.tr_programming)).assertCountEquals(0)
        rule.waitFor { rule.onAllNodesWithContentDescription(string(R.string.analysis)).fetchSemanticsNodes().isNotEmpty() }
        rule.onNodeWithContentDescription(string(R.string.analysis)).performClick()
        rule.onNodeWithContentDescription(string(R.string.tr_programming)).performClick()
        assertEquals(1, analysis)
        assertEquals(1, training)
    }

    @Test
    fun exportEditAndDeleteAreInTheMenu() {
        show()
        // closed: not in the bar
        rule.onAllNodesWithContentDescription(string(R.string.edit_team)).assertCountEquals(0)
        rule.onAllNodesWithContentDescription(string(R.string.delete_team)).assertCountEquals(0)
        rule.onNodeWithContentDescription(string(R.string.more_actions)).performClick()
        rule.onAllNodesWithText(string(R.string.share)).assertCountEquals(1)
        rule.onAllNodesWithText(string(R.string.save_to_file)).assertCountEquals(1)
        rule.onAllNodesWithText(string(R.string.delete_team)).assertCountEquals(1)
        rule.onAllNodesWithText(string(R.string.edit_team)).onFirst().performClick()
        assertEquals(1, edited)
    }

    /** Off at first, ticked in the menu and remembered by the core. */
    @Test
    fun theContactsGoInTheArchivesOnceTickedInTheMenu() {
        show()
        rule.onNodeWithContentDescription(string(R.string.more_actions)).performClick()
        val box = rule.onNodeWithText(string(R.string.include_contacts))
        box.assertIsOff()
        box.performClick()
        rule.waitFor { core { it.archiveContacts() } }
        box.assertIsOn()
    }
}
