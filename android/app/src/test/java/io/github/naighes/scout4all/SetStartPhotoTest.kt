package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.performClick
import io.github.naighes.scout4all.core.MatchForm
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The photo of the player in the list to choose the lineup from. */
@RunWith(RobolectricTestRunner::class)
class SetStartPhotoTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()
    private val match = core { it.saveMatch(team, null, MatchForm("Cortona", "2026-10-25T18:00:00+02:00", true, "6")) }

    private fun fixture(name: String) = javaClass.classLoader!!.getResourceAsStream(name)!!.use { it.readBytes() }

    @Test
    fun thePhotoOfAPlayerIsShownInTheListOfThePositions() {
        val first = core { it.team(team) }.players.first().id
        core { it.setPersonPhoto(first, core { c -> c.cropFace(fixture("solvay_one_face.jpg")) }) }
        rule.setContent { Scout4AllTheme { SetStartScreen(team, match, onBack = {}, onStarted = {}) } }
        rule.waitForText(string(R.string.start_set))
        rule.onNode(hasText("Italia") and hasClickAction()).performClick()
        rule.onNode(hasText(string(R.string.position_spoken, 1)) and hasClickAction()).performClick()
        rule.waitForText(nationalTeam.first().first)
        rule.waitFor { rule.onAllNodesWithTag(SHOWN_IMAGE, useUnmergedTree = true).fetchSemanticsNodes().size == 1 }
    }
}
