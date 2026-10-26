package io.github.naighes.scout4all

import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import org.junit.Assert.assertFalse
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** The badge of the setter beside the number, never on it: up to v0.10,
 * on the jerseys made smaller by a narrow court, it covered the second
 * digit of a number like 12. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class SetterBadgeTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }
    }

    @Test
    fun theBadgeLeavesTheNumberWhole() {
        val team = saveNationalTeam()
        val orro = core { it.team(team) }.players.first { it.name == "Alessia Orro" }.id
        core { it.savePlayer(team, orro, PlayerForm("Alessia Orro", RoleEnum.SETTER, "12")) }
        val match = startFirstSet(team)
        rule.setContent { Scout4AllTheme { ScoutingScreen(team, match, onBack = {}) } }
        rule.waitForText("Cortona", substring = true)
        rule.waitFor { rule.onAllNodes(hasTestTag(SETTER_BADGE), useUnmergedTree = true).fetchSemanticsNodes().size == 1 }
        val badge = rule.onNode(hasTestTag(SETTER_BADGE), useUnmergedTree = true).fetchSemanticsNode().bounds()
        val number = rule.onNode(hasText("12"), useUnmergedTree = true).fetchSemanticsNode().bounds()
        rule.shot("setter-badge-$screen")
        assertFalse("badge $badge over the number $number", badge.overlaps(number))
    }
}
