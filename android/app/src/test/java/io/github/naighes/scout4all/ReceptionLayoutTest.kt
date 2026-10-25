package io.github.naighes.scout4all

import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.unit.dp
import androidx.test.espresso.Espresso.pressBack
import io.github.naighes.scout4all.core.TeamSideEnum
import kotlin.math.abs
import kotlin.math.sign
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** We receive in every rotation, on every screen: the players spread on the
 * court, their names whole. The court is too small for everyone to be 48dp
 * apart, and in reception only the receivers are touched: the receivers are
 * 48dp apart; the players hiding behind (the setter, the opposite, the
 * middle blockers at the net) may be closer to a receiver, but beneath it,
 * so that the square of 48dp of the receiver is all its own, or closer to
 * one another. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class ReceptionLayoutTest(private val screen: Size, private val setterAt: Int) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0} P{1}")
        fun cases() = sizes.flatMap { size -> (1..6).map { arrayOf(size, it) } }

        // the outside hitters and the libero of `nationalTeam`
        private val receivers = listOf(7, 9, 6)
    }

    @Test
    fun theReception() {
        val team = saveNationalTeam()
        val match = startFirstSet(team, serving = TeamSideEnum.THEM, setterAt = setterAt)
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = {}) } }
        rule.waitForText("S$setterAt", substring = true)
        rule.shot("reception-P$setterAt-$screen")
        val crowded = rule.crowded()
        assertEquals(emptyList<String>(), rule.misfits(screen.scrolls) + rule.inaccessible().filterNot { it.startsWith("closer") })
        val pairs = crowded.map { (a, b) -> listOf(a, b).partition { it.number() in receivers } }
        assertEquals(emptyList<String>(), pairs.filter { it.first.size == 2 }.map { (r, _) -> r.joinToString { it.describe() } })
        val hidden = pairs.filter { it.first.size == 1 }.map { (r, other) -> r.single() to other.single() }
        // the edge of the square of the receiver, towards the other one
        hidden.forEach { (receiver, other) ->
            val (from, to) = receiver.bounds().center to other.bounds().center
            val side = with(receiver.layoutInfo.density) { 23.dp.toPx() }
            val touch = from + Offset(sign(to.x - from.x) * minOf(abs(to.x - from.x), side), sign(to.y - from.y) * minOf(abs(to.y - from.y), side))
            rule.onRoot().performTouchInput { click(touch) }
            val chosen = " · ${receiver.number()} · "
            rule.waitForText(chosen, substring = true)
            // the choice taken back (undo is off before the first event)
            // off the main thread: till then the court ignores touches
            pressBack()
            rule.waitFor({ "still $chosen in ${rule.texts()}" }) { rule.onAllNodesWithText(chosen, substring = true).fetchSemanticsNodes().isEmpty() }
        }
    }

    private fun SemanticsNode.number(): Int? = describe().substringBefore(' ').toIntOrNull()
}
