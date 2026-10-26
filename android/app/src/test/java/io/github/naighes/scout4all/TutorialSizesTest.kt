package io.github.naighes.scout4all

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
 * caption never over it. */
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
            }
            if (step.target == "stats") rule.shot("tutorial-$screen")
        }
        assertEquals(emptyList<String>(), wrong)
    }
}
