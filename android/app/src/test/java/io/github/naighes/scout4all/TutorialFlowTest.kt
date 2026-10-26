package io.github.naighes.scout4all

import androidx.compose.ui.test.click
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipe
import androidx.lifecycle.ViewModelProvider
import io.github.naighes.scout4all.core.MatchListing
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The tutorial of a new installation, on the real screens. */
@RunWith(RobolectricTestRunner::class)
class TutorialFlowTest {
    @get:Rule val rule = createAndroidComposeRule<MainActivity>()

    /** The terms accepted on a new installation: the tutorial starts. */
    private fun start(newInstallation: Boolean = true) {
        // as the app does, before the teams are listed (the tests don't)
        if (newInstallation) assertTrue(core { it.addExampleTeamIfNew() })
        rule.waitForText(string(R.string.accept_terms))
        touch(string(R.string.accept_terms))
    }

    private fun touch(text: String) = rule.onNode(hasText(text) and hasClickAction()).performClick()

    private fun caption(id: Int) = rule.waitForText(string(id))

    private val tutorial: Tutorial get() = ViewModelProvider(rule.activity)[TutorialViewModel::class.java].tutorial

    /** Under the veil only the target can be touched; the steps go on one
     * after the other to the scouting, as the captions say. */
    @Test
    fun fromTheTeamsToTheScoutingOfAMatchOfTheExampleTeam() {
        start()
        caption(R.string.tutorial_welcome)
        touch(string(R.string.tutorial_next))
        caption(R.string.tutorial_team)
        // under the veil: the settings don't open
        rule.onNode(hasContentDescription(string(R.string.settings)) and hasClickAction()).performClick()
        caption(R.string.tutorial_team)
        touch("Italy")
        caption(R.string.tutorial_matches)
        touch(string(R.string.matches))
        caption(R.string.tutorial_new_match)
        rule.onNode(hasContentDescription(string(R.string.new_match)) and hasClickAction()).performClick()
        caption(R.string.tutorial_match_fields)
        rule.waitForText("Brazil")
        // to read: the form can't be changed meanwhile
        touch(string(R.string.save))
        caption(R.string.tutorial_match_fields)
        touch(string(R.string.tutorial_next))
        caption(R.string.tutorial_save_match)
        touch(string(R.string.save))
        caption(R.string.tutorial_lineup)
        listOf("Alessia Orro", "Paola Egonu", "Ilaria Spirito", "Eleonora Fersino").forEach {
            rule.waitForText(it, substring = true)
        }
        touch(string(R.string.tutorial_next))
        caption(R.string.tutorial_start_set)
        touch(string(R.string.start_set))
        rule.waitForText(string(R.string.touch_the_player))
        caption(R.string.tutorial_score)
    }

    /** Every step can be done as its caption says, to the last one: the
     * match scouted as the script tells, the lineup changed. */
    @Test
    fun theWholeTutorialGoesToTheEnd() {
        start()
        playToTheEnd()
        rule.waitFor({ "the tutorial still to show" }) { !core { it.tutorialPending() } }
        val team = core { it.tutorialTeam() }
        val match = core { it.matches(team) }.single() as MatchListing.Match
        assertScouted(team, match)
    }

    private fun playToTheEnd() = rule.playTutorial(tutorial)

    /** The match as the tutorial scouts it: 2-0, the lineup changed. */
    private fun assertScouted(team: String, match: MatchListing.Match) {
        val state = core { it.scouting(team, match.id).state() }
        assertEquals(2, state.scoreUs.toInt())
        assertEquals(0, state.scoreThem.toInt())
        val numbers = state.court.map { it.number.toInt() }
        assertTrue("$numbers", numbers.containsAll(listOf(3, 24)))
        assertTrue("$numbers", numbers.none { it == 18 || it == 8 })
        assertEquals(3, state.court.single { it.setter }.number.toInt())
    }

    private fun tutorialFromTheSettings() {
        rule.onNode(hasContentDescription(string(R.string.settings)) and hasClickAction()).performClick()
        rule.waitForText(string(R.string.tutorial_detail))
        touch(string(R.string.tutorial))
    }

    /** Seen again the same day: on a new match, the first one kept. */
    @Test
    fun seenAgainFromTheSettingsItScoutsAnotherMatch() {
        start()
        playToTheEnd()
        // back from the scouting to the teams
        repeat(3) {
            rule.waitForIdle()
            rule.onNode(hasContentDescription(string(R.string.back)) and hasClickAction()).performClick()
        }
        rule.waitForText(string(R.string.teams))
        tutorialFromTheSettings()
        playToTheEnd()
        val team = core { it.tutorialTeam() }
        val matches = core { it.matches(team) }.map { it as MatchListing.Match }
        assertEquals(listOf("Brazil", "Brazil 2"), matches.map { it.opponent }.sorted())
        matches.forEach { assertScouted(team, it) }
        assertEquals(1, core { it.teams() }.size)
    }

    /** An update has no example team: seen from the settings, it gets one. */
    @Test
    fun onAnUpdateFromTheSettingsWithANewExampleTeam() {
        start(newInstallation = false)
        rule.waitForText(string(R.string.no_teams))
        tutorialFromTheSettings()
        playToTheEnd()
        assertEquals(1, core { it.teams() }.size)
        assertFalse(core { it.tutorialPending() })
    }

    @Test
    fun skippedItIsNotShownAgainAndTheAppGoesOnAsUsual() {
        start()
        caption(R.string.tutorial_welcome)
        touch(string(R.string.tutorial_skip))
        rule.waitFor({ "the tutorial still to show" }) { !core { it.tutorialPending() } }
        rule.onAllNodesWithText(string(R.string.tutorial_welcome)).assertCountEquals(0)
        touch("Italy")
        rule.waitForText(string(R.string.matches))
    }

    /** An update: no team of the example, no tutorial. */
    @Test
    fun anUpdateHasNoTutorial() {
        start(newInstallation = false)
        rule.waitForText(string(R.string.no_teams))
        rule.onAllNodesWithText(string(R.string.tutorial_welcome)).assertCountEquals(0)
        assertFalse(core { it.tutorialPending() })
        assertEquals(0, core { it.teams() }.size)
    }
}
