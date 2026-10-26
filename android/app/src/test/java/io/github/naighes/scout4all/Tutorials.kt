package io.github.naighes.scout4all

import androidx.activity.ComponentActivity
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.test.click
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.AndroidComposeTestRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipe
import androidx.lifecycle.ViewModelProvider
import org.junit.Assert.assertEquals

/** The tutorial of the activity of the test. */
val AndroidComposeTestRule<*, out ComponentActivity>.tutorial: Tutorial
    get() = ViewModelProvider(activity)[TutorialViewModel::class.java].tutorial

/** Plays each step of the tutorial where it shows its target, from the
 * welcome to the last one; `check` sees each step with its target, before
 * it's done. */
fun AndroidComposeTestRule<*, out ComponentActivity>.playTutorial(
    tutorial: Tutorial,
    check: (TutorialStep, Rect?) -> Unit = { _, _ -> },
) {
    waitForText(app.getString(R.string.tutorial_welcome))
    var steps = 0
    while (true) {
        val step = tutorial.step ?: break
        waitForText(app.getString(step.caption))
        val target = step.target
        if (target != null) waitFor({ "$target not shown for ${app.getString(step.caption)}" }) { target in tutorial.targets }
        waitForIdle()
        val at = target?.let { tutorial.targets[it] }
        check(step, at)
        when (step) {
            is TutorialStep.Touch -> onRoot().performTouchInput { click(at?.center ?: center) }
            is TutorialStep.SwipeUp -> onRoot().performTouchInput {
                val from = at?.center ?: center
                swipe(from, from.copy(y = from.y - 400f), durationMillis = 300)
            }
            is TutorialStep.Read -> onNode(hasText(app.getString(R.string.tutorial_next)) and hasClickAction()).performClick()
            is TutorialStep.Done -> onNode(hasText(app.getString(R.string.tutorial_finish)) and hasClickAction()).performClick()
        }
        waitForIdle()
        steps++
        check(steps < 100) { "the tutorial doesn't end" }
    }
    assertEquals(tutorialSteps("").size, steps)
}
