package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.onNodeWithText
import org.junit.Assert.assertEquals
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The photo of the player on the bench, when choosing who comes in: the
 * rows of the team's list, the initials without it. */
@RunWith(RobolectricTestRunner::class)
class ReplacementPhotoTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()
    private val bench = core { it.savePlayer(team, null, PlayerForm("Giulia Bianchi", RoleEnum.OUTSIDE_HITTER, "10")) }
    private val other = core { it.savePlayer(team, null, PlayerForm("Anna Neri", RoleEnum.MIDDLE_BLOCKER, "4")) }
    private val match = startFirstSet(team)

    private fun fixture(name: String) = javaClass.classLoader!!.getResourceAsStream(name)!!.use { it.readBytes() }

    @Test
    fun thePhotoOfThePlayerOnTheBenchIsShownWhenChoosingWhoComesIn() {
        core { it.setPersonPhoto(bench, core { c -> c.cropFace(fixture("solvay_one_face.jpg")) }) }
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = {}) } }
        rule.waitForText("Cortona", substring = true)
        rule.onNode(hasContentDescription(string(R.string.lineup)) and hasClickAction()).performClick()
        rule.waitFor { rule.onAllNodes(hasText(string(R.string.substitution_short)) and hasClickAction()).fetchSemanticsNodes().size == 1 }
        rule.onNode(hasText(string(R.string.substitution_short)) and hasClickAction()).performClick()
        // Ekaterina Antropova, outside hitter, number 9, on court
        rule.waitFor { rule.onAllNodes(hasText("9") and hasClickAction()).fetchSemanticsNodes().size == 1 }
        rule.onNode(hasText("9") and hasClickAction()).performClick()
        rule.waitForText("Giulia Bianchi")
        // the row is one touchable: its parts are in the tree apart
        rule.waitFor { rule.onAllNodesWithTag(SHOWN_IMAGE, useUnmergedTree = true).fetchSemanticsNodes().size == 1 }
    }

    /** One with the photo and two digits, one without and one digit: the
     * names start in the same place, the initials in place of the photo. */
    @Test
    fun theNamesOfWhoComesInAreAligned() {
        core { it.setPersonPhoto(bench, core { c -> c.cropFace(fixture("solvay_one_face.jpg")) }) }
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = {}) } }
        rule.waitForText("Cortona", substring = true)
        rule.onNode(hasContentDescription(string(R.string.lineup)) and hasClickAction()).performClick()
        rule.waitFor { rule.onAllNodes(hasText(string(R.string.substitution_short)) and hasClickAction()).fetchSemanticsNodes().size == 1 }
        rule.onNode(hasText(string(R.string.substitution_short)) and hasClickAction()).performClick()
        rule.waitFor { rule.onAllNodes(hasText("9") and hasClickAction()).fetchSemanticsNodes().size == 1 }
        rule.onNode(hasText("9") and hasClickAction()).performClick()
        rule.waitForText("Anna Neri")
        rule.waitForText("AN")
        val left = { name: String -> rule.onNodeWithText(name, useUnmergedTree = true).fetchSemanticsNode().boundsInRoot.left }
        assertEquals(left("Giulia Bianchi"), left("Anna Neri"))
    }
}
