package io.github.naighes.scout4all

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The photo of the player in the jersey on the court, the number over it. */
@RunWith(RobolectricTestRunner::class)
class CourtPhotoTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()
    private val match = startFirstSet(team)

    private fun fixture(name: String) = javaClass.classLoader!!.getResourceAsStream(name)!!.use { it.readBytes() }

    /** the court shows the players once the serve is made */
    private fun serve() = rule.onNode(hasContentDescription("positive") and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)

    @Test
    fun thePhotoOfAPlayerOnTheCourtIsShownInItsJersey() {
        val setter = core { c -> c.team(team).players.first { it.name == "Alessia Orro" }.id }
        core { it.setPersonPhoto(setter, core { c -> c.cropFace(fixture("solvay_one_face.jpg")) }) }
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = {}) } }
        rule.waitForText("Cortona", substring = true)
        serve()
        rule.waitFor { rule.onAllNodesWithTag(SHOWN_IMAGE, useUnmergedTree = true).fetchSemanticsNodes().size == 1 }
    }

    @Test
    fun withoutPhotosTheJerseysShowOnlyTheNumber() {
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = {}) } }
        rule.waitForText("Cortona", substring = true)
        serve()
        rule.waitFor { rule.onAllNodes(hasTestTag(SETTER_BADGE), useUnmergedTree = true).fetchSemanticsNodes().size == 1 }
        assert(rule.onAllNodesWithTag(SHOWN_IMAGE, useUnmergedTree = true).fetchSemanticsNodes().isEmpty())
    }
}
