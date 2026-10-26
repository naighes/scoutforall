package io.github.naighes.scout4all

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The veil of the tutorial: only its target can be touched. */
@RunWith(RobolectricTestRunner::class)
class TutorialTest {
    @get:Rule val rule = createComposeRule()

    private val touched = mutableListOf<String>()
    private var ended = 0
    private val tutorial = Tutorial().apply {
        start(
            listOf(
                TutorialStep.Touch("a", R.string.teams),
                TutorialStep.SwipeUp("b", until = "opened", caption = R.string.settings),
                TutorialStep.Read(null, R.string.lineup),
            ),
            onEnd = { ended++ },
        )
    }

    private fun show() = rule.setContent {
        Scout4AllTheme {
            CompositionLocalProvider(LocalTutorial provides tutorial) {
                Box(Modifier.fillMaxSize()) {
                    Column {
                        Button(onClick = { touched += "a" }, modifier = Modifier.tutorial("a")) { Text("A") }
                        Button(onClick = { touched += "b" }, modifier = Modifier.tutorial("b")) { Text("B") }
                    }
                    TutorialOverlay(tutorial)
                }
            }
        }
    }

    private fun touch(text: String) = rule.onNode(hasText(text) and hasClickAction()).performClick()

    @Test
    fun onlyTheTargetCanBeTouchedAndTouchingItGoesOn() {
        show()
        rule.waitForText(string(R.string.teams))
        touch("B")
        assertEquals(listOf<String>(), touched)
        assertEquals(0, tutorial.index)
        touch("A")
        assertEquals(listOf("a"), touched)
        assertEquals(1, tutorial.index)
        rule.waitForText(string(R.string.settings))
        // the target of a swipe: touched, it waits for what it opens
        touch("B")
        assertEquals(1, tutorial.index)
    }

    @Test
    fun aSwipeEndsWhenItsStepIsReachedAndAReadingWithNext() {
        show()
        touch("A")
        tutorial.reached("something else")
        assertEquals(1, tutorial.index)
        tutorial.reached("opened")
        rule.waitForText(string(R.string.lineup))
        // to read: nothing to touch but its buttons
        touch("A")
        assertEquals(listOf("a"), touched)
        assertEquals(0, ended)
        touch(string(R.string.tutorial_next))
        assertEquals(1, ended)
        assertEquals(null, tutorial.step)
        rule.waitForIdle()
        rule.onAllNodesWithText(string(R.string.tutorial_skip)).assertCountEquals(0)
        touch("B")
        assertEquals(listOf("a", "b"), touched)
    }

    @Test
    fun skippedItEndsOnceAndLeavesTheAppAsItWas() {
        show()
        rule.waitForText(string(R.string.tutorial_progress, 1, 3))
        touch(string(R.string.tutorial_skip))
        assertEquals(1, ended)
        tutorial.skip()
        tutorial.next()
        assertEquals(1, ended)
        rule.waitForIdle()
        touch("B")
        touch("A")
        assertEquals(listOf("b", "a"), touched)
    }
}
