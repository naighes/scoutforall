package io.github.naighes.scout4all

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import androidx.compose.ui.test.junit4.createComposeRule
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class MatchFormTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()
    private val saved = mutableListOf<String>()

    private fun show(match: String? = null) {
        rule.setContent { Scout4AllTheme { MatchFormScreen(team, match, onBack = {}, onSaved = { saved += it }) } }
        rule.waitForText(string(R.string.opponent))
    }

    private fun matches() = core { it.matches(team) }

    @Test
    fun aMatchWithoutOpponentIsNotSaved() {
        show()
        rule.save()
        rule.assertInvalid(string(R.string.opponent))
        assertEquals(emptyList<String>(), saved)
        assertEquals(0, matches().size)
    }

    @Test
    fun substitutionsThatAreNotANumberAreRejected() {
        show()
        rule.type(string(R.string.opponent), "Cortona")
        rule.type(string(R.string.max_substitutions), "six")
        rule.save()
        rule.assertInvalid(string(R.string.max_substitutions))
        rule.assertValid(string(R.string.opponent))
        assertEquals(0, matches().size)
    }

    @Test
    fun aNewMatchGoesOnWithItsId() {
        show()
        rule.type(string(R.string.opponent), "Cortona")
        rule.save()
        rule.waitFor { saved.size == 1 }
        assertEquals(1, matches().size)
        assertEquals("Cortona", core { it.matchForm(team, saved.single()) }.opponent)
    }

    @Test
    fun aMatchEdited() {
        val match = startFirstSet(team)
        show(match)
        rule.field(string(R.string.opponent)).assertTextContains("Cortona")
        rule.type(string(R.string.max_substitutions), "12")
        rule.save()
        rule.waitFor { saved.size == 1 }
        assertEquals("12", core { it.matchForm(team, match) }.maxSubstitutions)
    }

    @Test
    fun theNotesOfAMatchAreSaved() {
        show()
        rule.type(string(R.string.opponent), "Cortona")
        rule.type(string(R.string.notes), "windy gym")
        rule.save()
        rule.waitFor { saved.size == 1 }
        assertEquals("windy gym", core { it.matchForm(team, saved.single()) }.notes)
    }

    /** The row of the player in the roster. */
    private fun rosterRow(name: String) = rule.onNode(hasText(name, substring = true) and isToggleable())

    /** Fifteen players, one too many: one off, and the match is saved
     * with the other fourteen. */
    @Test
    fun aLargeTeamChoosesItsRoster() {
        (1..8).forEach { core { c -> c.savePlayer(team, null, PlayerForm("Extra $it", RoleEnum.OUTSIDE_HITTER, "${30 + it}")) } }
        show()
        rule.type(string(R.string.opponent), "Cortona")
        rosterRow("Extra 8").performScrollTo().assertIsOn()
        rule.save()
        rule.waitForText(string(R.string.roster, 15, 14))
        assertEquals(emptyList<String>(), saved)
        rosterRow("Extra 8").performScrollTo().performClick()
        rule.waitFor { runCatching { rosterRow("Extra 8").assertIsOff() }.isSuccess }
        rule.waitForText(string(R.string.roster, 14, 14))
        rule.save()
        rule.waitFor { saved.size == 1 }
        assertEquals(14, core { it.matchForm(team, saved.single()) }.roster?.size)
    }

    /** Who played in a set stays on the score sheet. */
    @Test
    fun whoPlayedStaysOnTheRoster() {
        val match = startFirstSet(team)
        show(match)
        rosterRow("Alessia Orro").performScrollTo().assertIsOn().assertIsNotEnabled()
    }
}
