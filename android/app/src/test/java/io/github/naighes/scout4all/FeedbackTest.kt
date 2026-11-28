package io.github.naighes.scout4all

import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** What the app tells: a note under the field to correct, a dialog for
 * what went wrong, as on the desktop. */
@RunWith(RobolectricTestRunner::class)
class FeedbackTest {
    @get:Rule val rule = createComposeRule()

    @Test
    fun aDialogTellsTheLevelAndTheTextUntilClosed() {
        var closed = 0
        rule.setContent { Scout4AllTheme { NoticeDialog(Notice.error("disk full")) { closed++ } } }
        rule.waitForText(string(R.string.notice_error))
        // the text as a sentence
        rule.onNodeWithText("Disk full").assertExists()
        rule.onNodeWithText(string(R.string.ok)).performClick()
        rule.waitFor { closed == 1 }
    }

    @Test
    fun eachLevelHasItsColorAndItsTitle() {
        val levels = Level.entries
        assertEquals(levels.size, levels.map { it.color }.toSet().size)
        assertEquals(levels.size, levels.map { it.title }.toSet().size)
        // none of them the accent: the actions are amber
        levels.forEach { assert(it.color != Scout4AllColors.primary) }
    }

    /** The name left empty: told under it, not in a dialog; written, gone. */
    @Test
    fun aFieldToCorrectIsToldUnderIt() {
        val team = saveTeam("Italia")
        rule.setContent { Scout4AllTheme { PlayerFormScreen(team, null, onBack = {}, onDone = {}) } }
        rule.waitForText(string(R.string.number))
        rule.save()
        rule.waitFor { rule.onAllNodesWithTag(FIELD_NOTE_TAG, useUnmergedTree = true).fetchSemanticsNodes().size == 1 }
        rule.onNodeWithText("Name cannot be empty", useUnmergedTree = true).assertExists()
        rule.onAllNodesWithTag(NOTICE_TAG).assertCountEquals(0)
        rule.type(string(R.string.name), "Paola Egonu")
        rule.waitFor { rule.onAllNodesWithTag(FIELD_NOTE_TAG, useUnmergedTree = true).fetchSemanticsNodes().isEmpty() }
    }
}
