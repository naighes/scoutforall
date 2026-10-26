package io.github.naighes.scout4all

import androidx.activity.ComponentActivity
import androidx.compose.runtime.Composable
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
import androidx.test.espresso.Espresso.pressBack
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Back from a form: at once when nothing changed, else once the user
 * agrees to lose the changes; by the arrow or by the back of the system. */
@RunWith(RobolectricTestRunner::class)
class FormBackTest {
    @get:Rule val rule = createAndroidComposeRule<ComponentActivity>()

    private var back = 0

    private fun show(label: Int, form: @Composable (onBack: () -> Unit) -> Unit) {
        rule.setContent { Scout4AllTheme { form { back++ } } }
        rule.waitForText(string(label))
    }

    private fun arrow() = rule.onNode(hasContentDescription(string(R.string.back)) and hasClickAction()).performClick()

    private fun touch(text: Int) = rule.onNode(hasText(string(text)) and hasClickAction()).performClick()

    @Test
    fun aFormNotChangedGoesBackAtOnce() {
        show(R.string.name) { TeamFormScreen(null, onBack = it, onSaved = {}) }
        arrow()
        rule.waitFor { back == 1 }
    }

    @Test
    fun theChangesOfATeamAreLostOnlyIfTheUserAgrees() {
        show(R.string.name) { TeamFormScreen(null, onBack = it, onSaved = {}) }
        rule.type(string(R.string.name), "Italia")
        arrow()
        rule.waitForText(string(R.string.discard_changes))
        touch(R.string.keep_editing)
        rule.waitForText("Italia")
        pressBack()
        rule.waitForText(string(R.string.discard_changes))
        touch(R.string.discard)
        rule.waitFor { back == 1 }
    }

    @Test
    fun theChangesOfAPlayerAskFirst() {
        val team = saveTeam("Italia")
        show(R.string.name) { PlayerFormScreen(team, null, onBack = it, onDone = {}) }
        rule.type(string(R.string.name), "Paola Egonu")
        arrow()
        rule.waitForText(string(R.string.discard_changes))
        assertEquals(0, back)
    }

    @Test
    fun theChangesOfAMatchAskFirst() {
        val team = saveTeam("Italia")
        show(R.string.opponent) { MatchFormScreen(team, null, onBack = it, onSaved = {}) }
        rule.type(string(R.string.opponent), "Cortona")
        arrow()
        rule.waitForText(string(R.string.discard_changes))
        assertEquals(0, back)
    }
}
