package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performClick
import io.github.naighes.scout4all.core.EvalEnum
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The events of a set and their correction. */
@RunWith(RobolectricTestRunner::class)
class SetEventsTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()
    private val match = startFirstSet(team)

    private fun show() {
        rule.setContent { Scout4AllTheme { SetScreen(team, match, 1, onBack = {}, onStats = {}) } }
    }

    private fun score() = core { it.scouting(team, match) }.state().let { it.scoreUs.toInt() to it.scoreThem.toInt() }

    private fun touch(text: String) {
        val touchable = hasText(text) and hasClickAction() and isEnabled()
        rule.waitFor { rule.onAllNodes(touchable).fetchSemanticsNodes().size == 1 }
        rule.onNode(touchable).performClick()
    }

    @Test
    fun noEvents() {
        show()
        rule.waitForText(string(R.string.no_events))
    }

    @Test
    fun theLastEventCorrected() {
        serve(team, match, EvalEnum.PERFECT, EvalEnum.PERFECT)
        show()
        rule.waitForText(string(R.string.score_to, 2, 0))
        touch(string(R.string.score_to, 2, 0))
        rule.waitForText(string(R.string.event_n, 2))
        touch("=")
        touch(string(R.string.save))
        rule.waitFor { score() == 1 to 1 }
        rule.waitForText(string(R.string.score_to, 1, 1))
    }

    @Test
    fun aCorrectionTheNextEventsDontFitIsNotSaved() {
        serve(team, match, EvalEnum.PERFECT, EvalEnum.PERFECT)
        show()
        rule.waitForText(string(R.string.score_to, 1, 0))
        touch(string(R.string.score_to, 1, 0))
        rule.waitForText(string(R.string.event_n, 1))
        // a serve error: then they serve, not us
        touch("=")
        touch(string(R.string.save))
        rule.waitFor { rule.onAllNodes(hasText(string(R.string.save)) and hasClickAction()).fetchSemanticsNodes().isNotEmpty() }
        rule.waitForIdle()
        assertEquals(2 to 0, score())
        rule.waitForText(string(R.string.event_n, 1))
    }
}
