package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.semantics.SemanticsActions
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** The scouting fits every screen at every step: never a scroll, no text
 * cut; the reception is in `ReceptionLayoutTest`. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class ScoutingLayoutTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }
    }

    private fun scout() {
        val team = saveNationalTeam()
        val match = startFirstSet(team)
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
        assertEquals(emptyList<String>(), rule.misfits(screen.scrolls) + rule.inaccessible())
    }

    @Test
    fun theServe() {
        scout()
        assertFits("theServe")
    }

    /** Their serve received: our court with its cells, as large as the
     * one of the serve. */
    @Test
    fun theReceptionZone() {
        scout()
        touch("=")
        touch("18")
        rule.waitForText("6A")
        assertFits("theReceptionZone")
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

    /** Our attack: the sets and how many blocked over the evaluations,
     * all without a scroll, as the serve. */
    @Test
    fun theAttackEvaluation() {
        scout()
        touch("+")
        touch("18")
        touch("defense")
        touch("+")
        // the setter can only attack
        touch("8")
        rule.waitForText("hole")
        assertFits("theAttackEvaluation")
    }

    /** Our block: the zones at the net, how many blocked. */
    @Test
    fun theBlockEvaluation() {
        scout()
        touch("+")
        touch("18")
        touch("block")
        rule.waitForText("hole")
        assertFits("theBlockEvaluation")
    }
}
