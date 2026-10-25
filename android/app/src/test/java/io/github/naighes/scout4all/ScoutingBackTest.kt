package io.github.naighes.scout4all

import androidx.activity.ComponentActivity
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
import androidx.test.espresso.Espresso.pressBack
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The back of the system in the scouting: what's open over it, or the
 * choice in the middle of an event, before the screen. */
@RunWith(RobolectricTestRunner::class)
class ScoutingBackTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private val team = saveNationalTeam()
    private val match = startFirstSet(team)
    private var back = 0

    private fun show() {
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = { back++ }) } }
        rule.waitForText("Cortona", substring = true)
    }

    private fun touch(text: String) {
        val touchable = hasText(text) and hasClickAction() and isEnabled()
        rule.waitFor({ "no \"$text\" in ${rule.texts()}" }) { rule.onAllNodes(touchable).fetchSemanticsNodes().size == 1 }
        rule.onNode(touchable).performClick()
    }

    private fun shown(text: String) = rule.onAllNodes(hasText(text)).fetchSemanticsNodes().isNotEmpty()

    @Test
    fun backTakesBackTheChoiceInTheMiddleOfAnEvent() {
        show()
        touch("+")
        rule.waitForText(string(R.string.touch_the_player))
        touch("Paola Egonu")
        rule.waitForText("18 · Paola Egonu")
        pressBack()
        rule.waitForText(string(R.string.touch_the_player))
        // the serve stays
        rule.waitForText("serve +", substring = true)
        assertEquals(0, back)
    }

    @Test
    fun backClosesTheChangesOfTheLineup() {
        show()
        rule.onNode(hasContentDescription(string(R.string.lineup)) and hasClickAction()).performClick()
        rule.waitForText(string(R.string.substitution_short))
        pressBack()
        rule.waitFor({ "still ${rule.texts()}" }) { !shown(string(R.string.substitution_short)) }
        assertEquals(0, back)
    }

    @Test
    fun backFromTheSetOverGoesBackToTheMatch() {
        show()
        repeat(25) { touch("#") }
        rule.waitForText(string(R.string.set_won, 1))
        pressBack()
        rule.waitFor { back == 1 }
    }
}
