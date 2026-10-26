package io.github.naighes.scout4all

import androidx.compose.runtime.Composable
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** The forms fit every screen, with the errors of the core too: they
 * scroll, the button to save is always whole, no text cut. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class FormsLayoutTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }
    }

    private fun assertFits(name: String, firstField: Int, content: @Composable () -> Unit) {
        rule.setContent { Scout4AllTheme { content() } }
        rule.waitForText(string(firstField))
        rule.save()
        rule.assertInvalid(string(firstField))
        rule.shot("form-$name-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
        rule.assertWhole(hasText(string(R.string.save)) and hasClickAction())
    }

    @Test
    fun theTeam() = assertFits("team", R.string.name) { TeamFormScreen(null, onBack = {}, onSaved = {}) }

    @Test
    fun thePlayer() {
        val team = saveTeam("Italia")
        assertFits("player", R.string.name) { PlayerFormScreen(team, null, onBack = {}, onDone = {}) }
    }

    @Test
    fun theMatch() {
        val team = saveTeam("Italia")
        assertFits("match", R.string.opponent) { MatchFormScreen(team, null, onBack = {}, onSaved = {}) }
    }
}
