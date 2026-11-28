package io.github.naighes.scout4all

import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
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

/** The scouting of both sides fits every screen at every step, as the one
 * of a side: never a scroll (but on the smallest), no text cut. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class BothSidesLayoutTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }
    }

    private fun scout() {
        val team = saveNationalTeam()
        val match = startFirstSetBoth(team)
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = {}) } }
        rule.waitForText("Arezzo", substring = true)
    }

    /** Touches what can be touched with `text`: a player by the number, not
 * where the last touch shows it. */
    private fun touch(text: String) {
        val touchable = hasText(text) and hasClickAction() and isEnabled()
        // the core reads the step off the main thread
        rule.waitUntil(WAIT_MS) { rule.onAllNodes(touchable).fetchSemanticsNodes().size == 1 }
        val node = rule.onNode(touchable)
        when {
            // maybe out of the window, in a scroll inside another
            screen.scrolls -> node.performSemanticsAction(SemanticsActions.OnClick)
            else -> node.performClick()
        }
        rule.waitForIdle()
    }

    private fun assertFits(name: String) {
        rule.shot("both-$name-$screen")
        assertEquals(emptyList<String>(), rule.misfits(screen.scrolls) + rule.inaccessible())
    }

    private fun swap() = rule.onNode(hasContentDescription(string(R.string.swap_side)) and hasClickAction())

    @Test
    fun theServe() {
        scout()
        assertFits("theServe")
    }

    /** Our serve evaluated: the side of B receives, spread on its court as
     * in `ReceptionLayoutTest`: the receivers 48dp apart, the players
     * hiding behind them may be closer. */
    @Test
    fun theSideOfBReceives() {
        scout()
        touch("+")
        rule.waitFor { runCatching { swap().assertIsEnabled() }.isSuccess }
        rule.shot("both-theSideOfBReceives-$screen")
        assertEquals(emptyList<String>(), rule.misfits(screen.scrolls) + rule.inaccessible().filterNot { it.startsWith("closer") })
        val receivers = listOf(7, 9, 6)
        val number = { n: SemanticsNode -> n.describe().substringBefore(' ').toIntOrNull() }
        assertEquals(
            emptyList<String>(),
            rule.crowded().filter { (a, b) -> number(a) in receivers && number(b) in receivers }.map { (a, b) -> "${a.describe()}, ${b.describe()}" },
        )
    }

    @Test
    fun theEventsOfThePlayerOfB() {
        scout()
        touch("+")
        touch("18")
        assertFits("theEventsOfThePlayerOfB")
    }

    /** B's reception evaluated: its next touch. */
    @Test
    fun theNextTouchOfB() {
        scout()
        touch("+")
        touch("18")
        touch("-")
        assertFits("theNextTouchOfB")
    }

    /** The side of A while B has the ball. */
    @Test
    fun theOtherSideOnScreen() {
        scout()
        touch("+")
        rule.waitFor { runCatching { swap().assertIsEnabled() }.isSuccess }
        swap().performClick()
        rule.waitForIdle()
        assertFits("theOtherSideOnScreen")
    }
}
