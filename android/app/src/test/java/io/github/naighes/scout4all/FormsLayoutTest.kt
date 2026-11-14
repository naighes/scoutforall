package io.github.naighes.scout4all

import androidx.compose.runtime.Composable
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.performClick
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** The forms fit every screen, with the errors of the core too: they
 * scroll, the button to save is always whole, no text cut. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class FormsLayoutTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }
    }

    private fun assertFits(name: String, firstField: Int, button: Int = R.string.save, content: @Composable () -> Unit) {
        rule.setContent { Scout4AllTheme { content() } }
        rule.waitForText(string(firstField))
        rule.onNode(hasText(string(button)) and hasClickAction()).performClick()
        rule.waitForIdle()
        rule.assertInvalid(string(firstField))
        rule.shot("form-$name-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
        rule.assertWhole(hasText(string(button)) and hasClickAction())
    }

    @Test
    fun theTeam() = assertFits("team", R.string.name) { TeamFormScreen(null, onBack = {}, onSaved = {}) }

    @Test
    fun thePlayer() {
        val team = saveTeam("Italia")
        assertFits("player", R.string.name) { PlayerFormScreen(team, null, onBack = {}, onDone = {}) }
    }

    @Test
    fun theMatch() {
        val team = saveTeam("Italia")
        assertFits("match", R.string.opponent, R.string.go_on) { MatchFormScreen(team, null, onBack = {}, onSaved = { _, _ -> }) }
    }

    /** The fields of a new match all shown: team A, a new opponent to
     * create, a new competition with its format. */
    @Test
    fun theMatchWithAllItsFields() {
        val team = saveTeam("Italia")
        saveTeam("Pallavolo Femminile Valdichiana")
        rule.setContent { Scout4AllTheme { MatchFormScreen(team, null, onBack = {}, onSaved = { _, _ -> }) } }
        rule.waitForText(string(R.string.opponent))
        rule.type(string(R.string.opponent), "Volley Club Montepulciano")
        rule.type(string(R.string.competition), "Serie C femminile – girone B")
        rule.waitForText(string(R.string.create_opponent))
        rule.shot("form-match-all-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
        rule.assertWhole(hasText(string(R.string.go_on)) and hasClickAction())
    }

    @Test
    fun theCompetition() {
        val team = saveNationalTeam()
        core { it.saveMatch(team, null, MatchForm("Cortona", "2026-10-25T18:00:00+02:00", true, "6", competition = "Serie A1")) }
        val id = core { it.competitions() }.single().id
        rule.setContent { Scout4AllTheme { CompetitionFormScreen(id, onBack = {}, onSaved = {}) } }
        rule.waitForText(string(R.string.edit_competition))
        rule.type(string(R.string.name), "")
        rule.onNode(hasText(string(R.string.save)) and hasClickAction()).performClick()
        rule.assertInvalid(string(R.string.name))
        rule.shot("form-competition-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
        rule.assertWhole(hasText(string(R.string.save)) and hasClickAction())
    }

    /** The roster of a team larger than the score sheet, with its error. */
    @Test
    fun theRoster() {
        val team = saveNationalTeam()
        (1..8).forEach { core { c -> c.savePlayer(team, null, PlayerForm("Giocatrice dal nome lunghissimo $it", RoleEnum.OUTSIDE_HITTER, "${30 + it}")) } }
        rule.setContent { Scout4AllTheme { MatchFormScreen(team, null, onBack = {}, onSaved = { _, _ -> }) } }
        rule.waitForText(string(R.string.opponent))
        rule.type(string(R.string.opponent), "Cortona")
        rule.goOn()
        rule.waitForText(string(R.string.roster, 15, 14))
        rule.save()
        // the error of the core in place of the hint
        rule.waitFor { rule.onAllNodesWithText(string(R.string.roster_hint)).fetchSemanticsNodes().isEmpty() }
        rule.shot("form-roster-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
        rule.assertWhole(hasText(string(R.string.save)) and hasClickAction())
    }
}
