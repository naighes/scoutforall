package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import io.github.naighes.scout4all.core.MatchForm
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The competitions listed, edited and removed (step 23). */
@RunWith(RobolectricTestRunner::class)
class CompetitionsTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()

    private fun matchIn(competition: String, opponent: String) = core {
        it.saveMatch(team, null, MatchForm(opponent, "2026-10-25T18:00:00+02:00", true, "6", competition = competition))
    }

    @Test
    fun noCompetitionsSaysWhereTheyAreCreated() {
        rule.setContent { Scout4AllTheme { CompetitionsScreen(onBack = {}, onCompetition = {}) } }
        rule.waitForText(string(R.string.no_competitions))
    }

    @Test
    fun theCompetitionsWithTheirMatches() {
        matchIn("Serie A1", "Conegliano")
        matchIn("Serie A1", "Novara")
        var opened: String? = null
        rule.setContent { Scout4AllTheme { CompetitionsScreen(onBack = {}, onCompetition = { opened = it }) } }
        rule.waitForText("Serie A1")
        rule.waitForText(app.resources.getQuantityString(R.plurals.matches_count, 2, 2))
        rule.onNodeWithText("Serie A1").performClick()
        assertEquals(core { it.competitions() }.single().id, opened)
    }

    @Test
    fun aCompetitionIsRemovedItsMatchesStay() {
        val match = matchIn("Serie A1", "Conegliano")
        rule.setContent { Scout4AllTheme { CompetitionsScreen(onBack = {}, onCompetition = {}) } }
        rule.waitForText("Serie A1")
        rule.onNode(hasContentDescription("${string(R.string.delete_competition)}: Serie A1") and hasClickAction()).performClick()
        rule.waitForText(string(R.string.delete_competition_text, "Serie A1", 1))
        rule.onNodeWithText(string(R.string.delete_forever)).performClick()
        rule.waitForText(string(R.string.no_competitions))
        assertEquals("", core { it.matchForm(team, match) }.competition)
        assertEquals(1, core { it.matches(team) }.size)
    }

    @Test
    fun aCompetitionEdited() {
        matchIn("Serie A1", "Conegliano")
        val id = core { it.competitions() }.single().id
        var saved = 0
        rule.setContent { Scout4AllTheme { CompetitionFormScreen(id, onBack = {}, onSaved = { saved++ }) } }
        rule.waitForText(string(R.string.edit_competition))
        rule.type(string(R.string.name), "Serie A1 femminile")
        rule.type(string(R.string.season), "2026/2027")
        rule.onNode(hasText(string(R.string.friendly_default)) and hasClickAction()).performClick()
        rule.save()
        rule.waitFor { saved == 1 }
        val form = core { it.competitionForm(id) }
        assertEquals("Serie A1 femminile", form.name)
        assertEquals("2026/2027", form.season)
        assertEquals(true, form.friendly)
    }

    /** An empty name, or the name and season of another one: rejected at
     * the name. */
    @Test
    fun aCompetitionWithoutNameIsNotSaved() {
        matchIn("Serie A1", "Conegliano")
        val id = core { it.competitions() }.single().id
        rule.setContent { Scout4AllTheme { CompetitionFormScreen(id, onBack = {}, onSaved = {}) } }
        rule.waitForText(string(R.string.edit_competition))
        rule.type(string(R.string.name), " ")
        rule.save()
        rule.assertInvalid(string(R.string.name))
        assertEquals("Serie A1", core { it.competitionForm(id) }.name)
    }
}
