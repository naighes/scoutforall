package io.github.naighes.scout4all

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.performClick
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.NextSetKind
import io.github.naighes.scout4all.core.TeamListing
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.RuntimeEnvironment

@RunWith(RobolectricTestRunner::class)
class TeamsScreenTest {
    @get:Rule val rule = createComposeRule()

    private val matches = mutableListOf<String>()
    private val resumed = mutableListOf<Pair<String, NextSetKind>>()

    private fun show(onTeam: (String) -> Unit = {}) = rule.setContent {
        Scout4AllTheme {
            TeamsScreen(
                TeamsViewModel(app),
                onTeam = onTeam,
                onNewTeam = {},
                onSettings = {},
                onMatch = { _, m -> matches += m },
                onResume = { _, m, next -> resumed += m to next },
            )
        }
    }

    @Test
    fun noTeams() {
        show()
        rule.waitForText(string(R.string.no_teams))
        rule.onNodeWithText(string(R.string.no_teams)).assertIsDisplayed()
    }

    @Test
    fun theTeamsWithTheirYearAndPlayers() {
        val cortona = saveTeam("Cortona")
        saveTeam("Arezzo", year = 2025)
        val opened = mutableListOf<String>()
        show { opened += it }
        rule.waitForText("Cortona")
        rule.onNodeWithText("Arezzo").assertIsDisplayed()
        rule.onNodeWithText("2026 · 0 players").assertIsDisplayed()
        rule.onAllNodesWithText(string(R.string.no_teams)).assertCountEquals(0)
        rule.onNodeWithText("Cortona").performClick()
        assertEquals(listOf(cortona), opened)
    }

    @Test
    fun withoutMatchesOnlyTheTeams() {
        saveTeam("Cortona")
        show()
        rule.waitForText("Cortona")
        rule.onAllNodesWithText(string(R.string.home_in_progress)).assertCountEquals(0)
        rule.onAllNodesWithText(string(R.string.home_recent)).assertCountEquals(0)
        rule.onNodeWithText(string(R.string.teams)).assertIsDisplayed()
    }

    @Test
    fun theMatchesInProgressGoOn() {
        val team = saveNationalTeam()
        val started = startFirstSet(team, "Cortona")
        val new = core { it.saveMatch(team, null, MatchForm("Arezzo", "2026-10-20T18:00:00+02:00", true, "6")) }
        show()
        rule.waitForText(string(R.string.home_in_progress))
        rule.onNodeWithText(string(R.string.resume)).performClick()
        rule.onNodeWithText(string(R.string.start)).performClick()
        assertEquals(
            listOf(started to NextSetKind.Resume(1u), new to NextSetKind.Start(1u)),
            resumed,
        )
        rule.onNodeWithText("Arezzo").performClick()
        assertEquals(listOf(new), matches)
    }

    /** The team of the tutorial comes after the matches: off screen, and
     * the step has to bring it in sight (a real installation has matches). */
    @Test
    fun theTutorialScrollsToItsTeamBelowTheMatches() {
        val team = saveNationalTeam()
        repeat(8) { winMatch(team, "Rival $it") }
        // a small screen: the five recent matches fill it
        RuntimeEnvironment.setQualifiers("w360dp-h400dp")
        val tutorial = Tutorial().apply {
            start(listOf(TutorialStep.Touch("team:$team", R.string.tutorial_team)), onEnd = {})
        }
        rule.setContent {
            Scout4AllTheme {
                CompositionLocalProvider(LocalTutorial provides tutorial) {
                    Box(Modifier.fillMaxSize()) {
                        TeamsScreen(TeamsViewModel(app), onTeam = {}, onNewTeam = {}, onSettings = {}, onMatch = { _, _ -> })
                        TutorialOverlay(tutorial)
                    }
                }
            }
        }
        rule.waitFor({ "team:$team not shown" }) { "team:$team" in tutorial.targets }
    }

    @Test
    fun theMatchesOverWithTheirSets() {
        val team = saveNationalTeam()
        val won = winMatch(team, "Serbia")
        show()
        rule.waitForText(string(R.string.home_recent))
        // read as the sets won and lost
        rule.onNodeWithText(string(R.string.sets_score, 3, 0)).assertIsDisplayed()
        rule.onAllNodesWithText(string(R.string.home_in_progress)).assertCountEquals(0)
        rule.onNodeWithText("Serbia").performClick()
        assertEquals(listOf(won), matches)
    }

    /** Without a logo, the initials in its place. */
    @Test
    fun aTeamWithoutALogoHasItsInitials() {
        saveTeam("Cortona Volley")
        show()
        rule.waitForText("Cortona Volley")
        rule.onNodeWithText("CV", useUnmergedTree = true).assertIsDisplayed()
    }

    /** The teams created by hand start pinned; a touch on the pin moves
     * the team below the pinned ones, saved for the next start. */
    @Test
    fun aTouchOnThePinMovesTheTeam() {
        saveTeam("Arezzo")
        saveTeam("Cortona")
        show()
        rule.waitForText("Arezzo")
        val unpin = string(R.string.unpin_team)
        val pin = string(R.string.pin_team)
        rule.onAllNodesWithContentDescription(unpin).assertCountEquals(2)
        // Arezzo, the first one
        rule.onAllNodesWithContentDescription(unpin)[0].performClick()
        rule.waitUntil { rule.onAllNodesWithContentDescription(pin).fetchSemanticsNodes().size == 1 }
        rule.onAllNodesWithContentDescription(unpin).assertCountEquals(1)
        val top = { name: String -> rule.onNodeWithText(name).fetchSemanticsNode().positionInRoot.y }
        assertTrue(top("Cortona") < top("Arezzo"))
        val saved = core { it.teams() }.map { (it as TeamListing.Team).name to it.pinned }
        assertEquals(listOf("Cortona" to true, "Arezzo" to false), saved)
    }
}
