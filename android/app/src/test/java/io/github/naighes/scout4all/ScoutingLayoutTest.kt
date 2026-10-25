package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.semantics.SemanticsActions
import io.github.naighes.scout4all.core.TeamSideEnum
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** The scouting fits every screen at every step: never a scroll, no text
 * cut. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class ScoutingLayoutTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }
    }

    private fun scout(serving: TeamSideEnum = TeamSideEnum.US) {
        val team = saveNationalTeam()
        val match = startFirstSet(team, serving = serving)
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = {}) } }
        rule.waitForText("Cortona", substring = true)
    }

    /** Touches what can be touched with `text`: a player by the number, not
 * where the last touch shows it. */
    private fun touch(text: String) {
        val touchable = hasText(text) and hasClickAction() and isEnabled()
        // the core reads the step off the main thread
        rule.waitUntil(5_000) { rule.onAllNodes(touchable).fetchSemanticsNodes().size == 1 }
        val node = rule.onNode(touchable)
        when {
            // maybe out of the window, in a scroll inside another
            screen.scrolls -> node.performSemanticsAction(SemanticsActions.OnClick)
            else -> node.performClick()
        }
        rule.waitForIdle()
    }

    private fun assertFits(name: String) {
        rule.shot("scouting-$name-$screen")
        assertEquals(emptyList<String>(), rule.misfits(screen.scrolls))
    }

    @Test
    fun theServe() {
        scout()
        assertFits("theServe")
    }

    @Test
    fun theirSideAfterTheServe() {
        scout()
        touch("+")
        assertFits("theirSideAfterTheServe")
    }

    @Test
    fun theEventsOfThePlayer() {
        scout()
        touch("+")
        touch("18")
        assertFits("theEventsOfThePlayer")
    }

    @Test
    fun theEvaluation() {
        scout()
        touch("+")
        touch("18")
        touch("defense")
        assertFits("theEvaluation")
    }

    @Test
    fun theReception() {
        scout(serving = TeamSideEnum.THEM)
        assertFits("theReception")
    }
}
