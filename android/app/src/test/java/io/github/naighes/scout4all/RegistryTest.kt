package io.github.naighes.scout4all

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.text.TextRange
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import io.github.naighes.scout4all.core.TeamListing
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The persons of the registry, in more teams (D5, D16), and the new
 * season of a team (D15): step 22. */
@RunWith(RobolectricTestRunner::class)
class RegistryTest {
    @get:Rule val rule = createComposeRule()

    private val italia = saveNationalTeam()
    private var done = 0

    private fun playerForm(team: String, player: String? = null) {
        rule.setContent { Scout4AllTheme { PlayerFormScreen(team, player, onBack = {}, onDone = { done++ }) } }
        rule.waitForText(string(R.string.number))
    }

    private fun idOf(team: String, name: String) = core { it.team(team) }.players.single { it.name == name }.id

    /** A new player written as a person of the registry, picked: the same
     * person in one more team, with the number of their team. */
    @Test
    fun aPersonOfTheRegistryJoinsAnotherTeam() {
        val u19 = saveTeam("Italia U19")
        playerForm(u19)
        rule.waitForText(string(R.string.name_or_existing))
        rule.field(string(R.string.name)).performTextReplacement("Egon")
        rule.onNode(hasText("Paola Egonu (Italia 2026)") and !hasSetTextAction()).performClick()
        rule.waitForText(string(R.string.in_registry))
        rule.save()
        rule.waitFor { done == 1 }
        val player = core { it.team(u19) }.players.single()
        assertEquals("Paola Egonu", player.name)
        assertEquals(18, player.number.toInt())
        assertEquals(idOf(italia, "Paola Egonu"), player.id)
        assertEquals(listOf("Italia (2026)"), core { it.otherTeams(u19, player.id) })
    }

    /** Picked, the name has the cursor at its end (seen on the phone: it
     * stayed after the letters written, in the middle of the name). */
    @Test
    fun thePickedNameHasTheCursorAtItsEnd() {
        val u19 = saveTeam("Italia U19")
        playerForm(u19)
        rule.field(string(R.string.name)).performTextReplacement("Egon")
        rule.onNode(hasText("Paola Egonu (Italia 2026)") and !hasSetTextAction()).performClick()
        rule.waitForText(string(R.string.in_registry))
        val selection = rule.field(string(R.string.name)).fetchSemanticsNode().config[SemanticsProperties.TextSelectionRange]
        assertEquals(TextRange("Paola Egonu".length), selection)
    }

    /** The name written again after the pick: a new person. */
    @Test
    fun anotherNameIsAnotherPerson() {
        val u19 = saveTeam("Italia U19")
        playerForm(u19)
        rule.field(string(R.string.name)).performTextReplacement("Egon")
        rule.onNode(hasText("Paola Egonu (Italia 2026)") and !hasSetTextAction()).performClick()
        rule.waitForText(string(R.string.in_registry))
        rule.type(string(R.string.name), "Paola Egonu Jr")
        rule.waitForText(string(R.string.name_or_existing))
        rule.save()
        rule.waitFor { done == 1 }
        val player = core { it.team(u19) }.players.single()
        assertEquals("Paola Egonu Jr", player.name)
        assert(player.id != idOf(italia, "Paola Egonu"))
    }

    /** Edited, the player says where else they play. */
    @Test
    fun aPlayerSaysWhereElseTheyPlay() {
        val u19 = saveTeam("Italia U19")
        val egonu = idOf(italia, "Paola Egonu")
        core { it.savePlayer(u19, null, PlayerForm("Paola Egonu", RoleEnum.OPPOSITE_HITTER, "1", person = egonu)) }
        playerForm(italia, egonu)
        rule.waitForText(string(R.string.plays_also_in, "Italia U19 (2026)"))
    }

    /** One question, two answers: out of the team (kept in the registry),
     * or out of the registry, so of every team. */
    @Test
    fun aPlayerIsRemovedFromTheTeamOrFromTheRegistry() {
        val u19 = saveTeam("Italia U19")
        val egonu = idOf(italia, "Paola Egonu")
        core { it.savePlayer(u19, null, PlayerForm("Paola Egonu", RoleEnum.OPPOSITE_HITTER, "1", person = egonu)) }
        playerForm(italia, egonu)
        rule.onNodeWithContentDescription(string(R.string.remove_player)).performClick()
        rule.waitForText(string(R.string.delete_person_text, 18), substring = true)
        rule.onNode(hasText(string(R.string.delete_person)) and hasClickAction()).performClick()
        rule.waitFor { done == 1 }
        assert(core { it.team(italia) }.players.none { it.name == "Paola Egonu" })
        assertEquals(emptyList<String>(), core { it.team(u19) }.players.map { it.name })
        assertEquals(emptyList<String>(), core { it.existingPlayers(u19) }.filter { it.name == "Paola Egonu" }.map { it.name })
    }

    @Test
    fun aPlayerRemovedFromTheTeamStaysInTheRegistry() {
        val u19 = saveTeam("Italia U19")
        playerForm(italia, idOf(italia, "Paola Egonu"))
        rule.onNodeWithContentDescription(string(R.string.remove_player)).performClick()
        rule.onNode(hasText(string(R.string.remove_from_team)) and hasClickAction()).performClick()
        rule.waitFor { done == 1 }
        assert(core { it.team(italia) }.players.none { it.name == "Paola Egonu" })
        assertEquals(listOf("Paola Egonu"), core { it.existingPlayers(u19) }.filter { it.name == "Paola Egonu" }.map { it.name })
    }

    private fun tile(name: String) = rule.onNode(hasText(name, substring = true) and isToggleable())

    /** The new season: the form the year after, the players all chosen; one
     * left, one with another number. The old team stays as it was. */
    @Test
    fun aNewSeasonTakesThePlayersChosen() {
        var created: String? = null
        rule.setContent { Scout4AllTheme { NewSeasonScreen(italia, onBack = {}, onSaved = { created = it }) } }
        rule.waitForText(string(R.string.new_season))
        rule.field(string(R.string.year)).assertExists()
        rule.waitForText("2027")
        rule.goOn()
        rule.waitForText(string(R.string.season_count, 7))
        tile("Monica De Gennaro").performScrollTo().assertIsOn().performClick()
        rule.waitForText(string(R.string.season_count, 6))
        tile("Monica De Gennaro").performScrollTo().assertIsOff()
        rule.onNode(hasContentDescription("8 Alessia Orro: ${string(R.string.roster_correct)}") and hasClickAction())
            .performScrollTo().performClick()
        rule.type(string(R.string.number), "1")
        rule.onNode(hasText(string(R.string.ok)) and hasClickAction()).performClick()
        rule.waitFor { runCatching { rule.onNode(hasContentDescription("1 Alessia Orro: ${string(R.string.roster_correct)}")).assertExists() }.isSuccess }
        rule.save()
        rule.waitFor { created != null }
        val season = core { it.team(created!!) }
        assertEquals(2027, season.year.toInt())
        assertEquals(6, season.players.size)
        assertEquals(1, season.players.single { it.name == "Alessia Orro" }.number.toInt())
        assertEquals(idOf(italia, "Alessia Orro"), season.players.single { it.name == "Alessia Orro" }.id)
        assertEquals(8, core { it.team(italia) }.players.single { it.name == "Alessia Orro" }.number.toInt())
        assertEquals(2, core { it.teams() }.filterIsInstance<TeamListing.Team>().count { it.name == "Italia" })
    }

    /** A number of another player chosen: the dialog says why. */
    @Test
    fun aNewSeasonDoesNotGiveANumberTwice() {
        rule.setContent { Scout4AllTheme { NewSeasonScreen(italia, onBack = {}, onSaved = {}) } }
        rule.waitForText(string(R.string.new_season))
        rule.goOn()
        rule.onNode(hasContentDescription("8 Alessia Orro: ${string(R.string.roster_correct)}") and hasClickAction())
            .performScrollTo().performClick()
        rule.type(string(R.string.number), "7")
        rule.onNode(hasText(string(R.string.ok)) and hasClickAction()).performClick()
        rule.assertInvalid(string(R.string.number))
    }
}
