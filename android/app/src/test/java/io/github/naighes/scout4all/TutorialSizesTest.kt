package io.github.naighes.scout4all

import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** The tutorial on every screen: each target in the window and whole, the
 * caption never over it, its finger or the score. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class TutorialSizesTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createAndroidComposeRule<MainActivity>()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }
    }

    @Test
    fun everyStepShowsItsTargetUncovered() {
        core { it.addExampleTeamIfNew() }
        rule.waitForText(string(R.string.accept_terms))
        rule.onNode(hasText(string(R.string.accept_terms)) and hasClickAction()).performClick()
        val window = rule.onRoot().fetchSemanticsNode().boundsInRoot
        val wrong = mutableListOf<String>()
        rule.playTutorial(rule.tutorial) { step, target ->
            val name = string(step.caption)
            if (target != null && step !is TutorialStep.Read) {
                val caption = rule.onNode(hasText(name)).fetchSemanticsNode().boundsInRoot
                if (target.intersect(window) != target) wrong += "out of the window: $name"
                if (caption.overlaps(target)) wrong += "under the caption: $name"
                // the box of the caption, round its words
                val box = rule.onNode(hasText(name), useUnmergedTree = true).fetchSemanticsNode().parent?.boundsInRoot ?: caption
                if (box.overlaps(finger(target))) wrong += "finger under the caption: $name"
            }
            val score = rule.tutorial.targets["score"]
            if (score != null && step.target != "score" && step !is TutorialStep.Done) {
                val caption = rule.onNode(hasText(name)).fetchSemanticsNode().boundsInRoot
                if (caption.overlaps(score)) wrong += "score under the caption: $name"
            }
            if (step.target == "stats") rule.shot("tutorial-$screen")
        }
        assertEquals(emptyList<String>(), wrong)
    }

    private fun finger(target: Rect): Rect = with(rule.density) {
        val size = Finger.roundToPx()
        val at = fingerAt(target.inflate(HoleMargin.toPx()), HoleMargin.toPx(), size)
        Rect(at.x.toFloat(), at.y.toFloat(), (at.x + size).toFloat(), (at.y + size).toFloat())
    }
}
