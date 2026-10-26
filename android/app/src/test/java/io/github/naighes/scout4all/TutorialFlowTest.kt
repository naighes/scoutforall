package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.performClick
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
        rule.waitFor({ "the tutorial still to show" }) { !core { it.tutorialPending() } }
        rule.onAllNodesWithText(string(R.string.tutorial_skip)).assertCountEquals(0)
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
