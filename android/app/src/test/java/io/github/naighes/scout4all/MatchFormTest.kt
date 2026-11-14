package io.github.naighes.scout4all

import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.assertIsOff
import androidx.compose.ui.test.assertIsOn
import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onFirst
import io.github.naighes.scout4all.core.CompetitionFormat
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.MatchListing
import io.github.naighes.scout4all.core.Stage
import io.github.naighes.scout4all.core.TeamListing
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
        rule.setContent { Scout4AllTheme { MatchFormScreen(team, match, onBack = {}, onSaved = { _, m -> saved += m }) } }
        rule.waitForText(string(R.string.opponent))
    }

    private fun matches() = core { it.matches(team) }

    /** The day of a new match written as every field of a day: in the
     * language of the phone, the same style as the birth of a player. */
    @Test
    fun theDayIsWrittenAsInEveryForm() {
        show()
        // read-only, a tap opens the calendar: found by what it says, whole
        // (not the weekday before it, as it was)
        val long = java.time.format.DateTimeFormatter.ofLocalizedDate(java.time.format.FormatStyle.LONG)
        rule.onAllNodesWithText(java.time.LocalDate.now().format(long)).onFirst().performScrollTo()
            .assertTextContains(string(R.string.date), substring = true)
    }

    @Test
    fun aMatchWithoutOpponentIsNotSaved() {
        show()
        rule.goOn()
        rule.assertInvalid(string(R.string.opponent))
        assertEquals(emptyList<String>(), saved)
        assertEquals(0, matches().size)
    }

    @Test
    fun substitutionsThatAreNotANumberAreRejected() {
        show()
        rule.type(string(R.string.opponent), "Cortona")
        rule.type(string(R.string.max_substitutions), "six")
        rule.goOn()
        rule.assertInvalid(string(R.string.max_substitutions))
        rule.assertValid(string(R.string.opponent))
        assertEquals(0, matches().size)
    }

    @Test
    fun aNewMatchGoesOnWithItsId() {
        show()
        rule.type(string(R.string.opponent), "Cortona")
        rule.goOn()
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
        rule.goOn()
        rule.save()
        rule.waitFor { saved.size == 1 }
        assertEquals("12", core { it.matchForm(team, match) }.maxSubstitutions)
    }

    @Test
    fun theNotesOfAMatchAreSaved() {
        show()
        rule.type(string(R.string.opponent), "Cortona")
        rule.type(string(R.string.notes), "windy gym")
        rule.goOn()
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
        rule.goOn()
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
        rule.goOn()
        rosterRow("Alessia Orro").performScrollTo().assertIsOn().assertIsNotEnabled()
    }

    /** Back from the roster: the fields as they were, the roster as chosen. */
    @Test
    fun backFromTheRosterToTheFields() {
        show()
        rule.type(string(R.string.opponent), "Cortona")
        rule.goOn()
        rosterRow("Alessia Orro").performScrollTo().performClick()
        rule.waitFor { runCatching { rosterRow("Alessia Orro").assertIsOff() }.isSuccess }
        rule.onNode(hasContentDescription(string(R.string.back)) and hasClickAction()).performClick()
        rule.field(string(R.string.opponent)).assertTextContains("Cortona")
        rule.goOn()
        rosterRow("Alessia Orro").performScrollTo().assertIsOff()
    }

    private fun formOf(match: String, of: String = team) = core { it.matchForm(of, match) }

    /** Saves the match from the fields, the roster as offered. */
    private fun saveFromTheFields() {
        rule.goOn()
        rule.save()
        rule.waitFor({ "not saved: ${rule.texts()}" }) { saved.size == 1 }
    }

    /** The menu with `label` opened, `option` picked. */
    private fun choose(label: String, option: String) {
        // read-only: no text to set
        rule.onAllNodesWithText(label).onFirst().performScrollTo().performClick()
        rule.onNode(hasText(option) and hasClickAction() and !hasSetTextAction()).performClick()
        rule.waitForIdle()
    }

    private fun tap(text: String) {
        rule.onNode(hasText(text) and hasClickAction()).performScrollTo().performClick()
        rule.waitForIdle()
    }

    /** Team B written as the name of a team, whatever the case: the match
     * is against that team, listed in both. */
    @Test
    fun anOpponentWrittenAsATeamIsThatTeam() {
        val cortona = saveTeam("Cortona")
        show()
        rule.type(string(R.string.opponent), "cortona ")
        // a team already: nothing to create
        rule.onAllNodesWithText(string(R.string.create_opponent)).assertCountEquals(0)
        saveFromTheFields()
        assertEquals(cortona, formOf(saved.single()).opponentTeam)
        val listed = core { it.matches(cortona) }.single() as MatchListing.Match
        assertEquals(team, listed.scoutedBy)
    }

    /** Of two teams with the same name, the one picked. */
    @Test
    fun theTeamPickedOfTwoNamesakes() {
        saveTeam("Cortona", 2026)
        val older = saveTeam("Cortona", 2025)
        show()
        rule.field(string(R.string.opponent)).performTextReplacement("Cort")
        rule.onNode(hasText("Cortona (2025)") and !hasSetTextAction()).performClick()
        rule.waitForIdle()
        saveFromTheFields()
        assertEquals(older, formOf(saved.single()).opponentTeam)
    }

    /** A new name stays a name, unless created as a team: then a team
     * without players. */
    @Test
    fun aNewOpponentIsCreatedAsATeam() {
        show()
        rule.type(string(R.string.opponent), "Pisa")
        tap(string(R.string.create_opponent))
        saveFromTheFields()
        val pisa = core { it.teams() }.filterIsInstance<TeamListing.Team>().single { it.name == "Pisa" }
        assertEquals(pisa.id, formOf(saved.single()).opponentTeam)
        assertEquals(0u, pisa.players)
    }

    @Test
    fun aNameStaysAName() {
        show()
        rule.type(string(R.string.opponent), "Pisa")
        saveFromTheFields()
        assertEquals(null, formOf(saved.single()).opponentTeam)
        assertEquals(listOf("Italia"), core { it.teams() }.filterIsInstance<TeamListing.Team>().map { it.name })
    }

    /** The court, the competition new with its format, friendly, stage and
     * round: saved as chosen. */
    @Test
    fun aNeutralCourtAndANewCompetition() {
        show()
        rule.type(string(R.string.opponent), "Cortona")
        tap(string(R.string.neutral_game))
        rule.type(string(R.string.competition), "Coppa Italia")
        val cup = core { it.competitionFormats() }.single { it.value == CompetitionFormat.CUP }.name
        choose(string(R.string.competition_format), cup)
        tap(string(R.string.friendly))
        val playoff = core { it.stages() }.single { it.value == Stage.PLAYOFF }.name
        choose(string(R.string.stage), playoff)
        rule.type(string(R.string.round), "Semifinale – gara 2")
        saveFromTheFields()
        val form = formOf(saved.single())
        assertEquals(true, form.neutral)
        assertEquals("Coppa Italia", form.competition)
        assertEquals(true, form.friendly)
        assertEquals(Stage.PLAYOFF, form.stage)
        assertEquals("Semifinale – gara 2", form.round)
        assertEquals(CompetitionFormat.CUP, core { it.competitions() }.single().format)
    }

    /** A competition written as one there is: that one, with its friendly;
     * no format to choose. */
    @Test
    fun aCompetitionThereIsGivesItsFriendly() {
        val first = core {
            it.saveMatch(team, null, MatchForm("Pisa", "2026-10-25T18:00:00+02:00", true, "6", competition = "Torneo di Natale"))
        }
        val torneo = core { it.competitions() }.single()
        core { it.saveCompetition(torneo.id, it.competitionForm(torneo.id).copy(friendly = true)) }
        show()
        rule.type(string(R.string.opponent), "Cortona")
        rule.type(string(R.string.competition), "torneo di natale")
        rule.onAllNodesWithText(string(R.string.competition_format)).assertCountEquals(0)
        rule.onNode(hasText(string(R.string.friendly)) and isToggleable()).assertIsOn()
        saveFromTheFields()
        assertEquals(torneo.id, formOf(saved.single()).competitionId)
        assertEquals(true, formOf(saved.single()).friendly)
        assertEquals(1, core { it.competitions() }.size)
        assertEquals(false, formOf(first).friendly)
    }

    /** A new match of another team A: its players on the score sheet,
     * saved as its match. */
    @Test
    fun aNewMatchOfAnotherTeam() {
        val cortona = saveTeam("Cortona")
        nationalTeam.forEach { (name, role, number) ->
            core { it.savePlayer(cortona, null, PlayerForm("$name C", role, number.toString())) }
        }
        var savedAs: String? = null
        rule.setContent {
            Scout4AllTheme { MatchFormScreen(team, null, onBack = {}, onSaved = { a, m -> savedAs = a; saved += m }) }
        }
        rule.waitForText(string(R.string.opponent))
        choose(string(R.string.team_a), "Cortona (2026)")
        rule.type(string(R.string.opponent), "Pisa")
        rule.goOn()
        rosterRow("Alessia Orro C").performScrollTo().assertIsOn()
        rule.onAllNodesWithText("Alessia Orro").assertCountEquals(0)
        rule.save()
        rule.waitFor { saved.size == 1 }
        assertEquals(cortona, savedAs)
        assertEquals("Pisa", formOf(saved.single(), cortona).opponent)
        assertEquals(0, matches().size)
    }

    /** Team A isn't chosen in a match being edited. */
    @Test
    fun theTeamOfAMatchEditedStays() {
        saveTeam("Cortona")
        val match = startFirstSet(team)
        show(match)
        rule.onAllNodes(hasSetTextAction() and hasText(string(R.string.team_a))).assertCountEquals(0)
    }

    /** The pencil of the player's tile. */
    private fun pencil(number: Int, name: String) =
        rule.onNode(hasContentDescription("$number $name: ${string(R.string.roster_correct)}") and hasClickAction())

    /** A number corrected for the match only, even for who played: the
     * team keeps its own. */
    @Test
    fun aNumberOfTheScoreSheetIsCorrected() {
        val match = startFirstSet(team)
        show(match)
        rule.goOn()
        pencil(8, "Alessia Orro").performScrollTo().performClick()
        rule.waitForText("8 Alessia Orro")
        rule.type(string(R.string.number), "88")
        rule.onNode(hasText(string(R.string.ok)) and hasClickAction()).performClick()
        rule.waitFor { rule.onAllNodesWithText("8 Alessia Orro").fetchSemanticsNodes().isEmpty() }
        pencil(88, "Alessia Orro").assertExists()
        rule.save()
        rule.waitFor { saved.size == 1 }
        val players = core { it.matchRoster(team, match, it.matchForm(team, match), null) }.players
        assertEquals(88, players.single { it.name == "Alessia Orro" }.number.toInt())
        assertEquals(8, core { it.team(team) }.players.single { it.name == "Alessia Orro" }.number.toInt())
    }

    /** The number of another player on the score sheet: the dialog stays,
     * with why, and nothing changes. */
    @Test
    fun aNumberOnTheScoreSheetIsNotGivenTwice() {
        show()
        rule.type(string(R.string.opponent), "Cortona")
        rule.goOn()
        pencil(8, "Alessia Orro").performScrollTo().performClick()
        rule.waitForText("8 Alessia Orro")
        rule.type(string(R.string.number), "7")
        rule.onNode(hasText(string(R.string.ok)) and hasClickAction()).performClick()
        rule.assertInvalid(string(R.string.number))
        rule.onNode(hasText(string(R.string.cancel)) and hasClickAction()).performClick()
        rule.waitFor { rule.onAllNodesWithText("8 Alessia Orro").fetchSemanticsNodes().isEmpty() }
        pencil(8, "Alessia Orro").assertExists()
    }
}
