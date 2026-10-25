package io.github.naighes.scout4all

import androidx.compose.runtime.Composable
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.MatchForm
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** The other screens fit every screen too: they may scroll, no text cut,
 * the button that leads on always whole. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class ScreensLayoutTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }
    }

    private fun assertFits(name: String, shown: String, button: String? = null, content: @Composable () -> Unit) {
        rule.setContent { Scout4AllTheme { content() } }
        rule.waitForText(shown)
        rule.shot("screen-$name-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
        button?.let { rule.assertWhole(hasText(it) and hasClickAction()) }
    }

    @Test
    fun theTeams() {
        saveNationalTeam()
        assertFits("teams", "Italia") {
            TeamsScreen(TeamsViewModel(app), onTeam = {}, onNewTeam = {}, onSettings = {}, onMatch = { _, _ -> })
        }
    }

    @Test
    fun theTeam() {
        val team = saveNationalTeam()
        assertFits("team", "Ekaterina Antropova") {
            TeamScreen(team, onBack = {}, onEdit = {}, onNewPlayer = {}, onPlayer = {}, onNewMatch = {}, onMatch = {})
        }
    }

    @Test
    fun theMatch() {
        val team = saveNationalTeam()
        val match = startFirstSet(team)
        serve(team, match, EvalEnum.PERFECT)
        assertFits("match", string(R.string.match_stats), string(R.string.resume_set_n, 1)) {
            MatchScreen(team, match, onBack = {}, onEdit = {}, onStartSet = {}, onResumeSet = {}, onSet = {}, onStats = {})
        }
    }

    @Test
    fun theStartOfTheSet() {
        val team = saveNationalTeam()
        val match = core { it.saveMatch(team, null, MatchForm("Cortona", "2026-10-25T18:00:00+02:00", true, "6")) }
        assertFits("start-set", string(R.string.start_set), string(R.string.start_set)) {
            SetStartScreen(team, match, onBack = {}, onStarted = {})
        }
    }

    @Test
    fun theEventsOfTheSet() {
        val team = saveNationalTeam()
        val match = startFirstSet(team)
        serve(team, match, EvalEnum.PERFECT, EvalEnum.POSITIVE)
        assertFits("set", string(R.string.statistics)) {
            SetScreen(team, match, 1, onBack = {}, onStats = {})
        }
    }

    @Test
    fun theSettings() = assertFits("settings", string(R.string.send_statistics)) {
        SettingsScreen(onBack = {}, onTerms = {}, onAbout = {})
    }

    @Test
    fun theAbout() = assertFits("about", string(R.string.report_problem)) { AboutScreen(onBack = {}) }

    @Test
    fun theTerms() = assertFits("terms", string(R.string.accept_terms), string(R.string.accept_terms)) {
        TermsScreen(onAccept = {}, onBack = {})
    }
}
