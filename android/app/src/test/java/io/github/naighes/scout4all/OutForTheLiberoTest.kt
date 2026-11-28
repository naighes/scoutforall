package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performClick
import io.github.naighes.scout4all.core.EventTypeEnum
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** Who is out while the libero plays in their place can go out too, though
 * not on court: a row of its own under the court, told apart. */
@RunWith(RobolectricTestRunner::class)
class OutForTheLiberoTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()
    private val bench = core { it.savePlayer(team, null, PlayerForm("Giulia Bianchi", RoleEnum.MIDDLE_BLOCKER, "10")) }

    // we serve, the setter in 1: the libero in for the middle blocker in 6
    private val match = startFirstSet(team)

    // a choice halfway is not saved
    private val out = core { c ->
        c.scouting(team, match).use { s ->
            s.chooseEvent(EventTypeEnum.R)
            s.playerChoices().single { it.outForTheLibero }
        }
    }

    private fun substitution() {
        rule.onNode(hasContentDescription(string(R.string.lineup)) and hasClickAction()).performClick()
        rule.waitFor { rule.onAllNodes(hasText(string(R.string.substitution_short)) and hasClickAction()).fetchSemanticsNodes().size == 1 }
        rule.onNode(hasText(string(R.string.substitution_short)) and hasClickAction()).performClick()
    }

    private fun outRow(name: String) =
        hasText(name, substring = true) and hasText(string(R.string.out_for_the_libero), substring = true) and hasClickAction()

    @Test
    fun theOneOutForTheLiberoIsSubstituted() {
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = {}) } }
        rule.waitForText("Cortona", substring = true)
        substitution()
        rule.waitFor { rule.onAllNodes(outRow(out.name)).fetchSemanticsNodes().size == 1 }
        rule.onNode(outRow(out.name)).performClick()
        rule.waitForText("Giulia Bianchi")
        rule.onNode(hasText("Giulia Bianchi") and hasClickAction()).performClick()
        // the libero still on court, out now for who came in
        substitution()
        rule.waitFor { rule.onAllNodes(outRow("Giulia Bianchi")).fetchSemanticsNodes().size == 1 }
    }
}
