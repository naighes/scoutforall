package io.github.naighes.scout4all

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.isPopup
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performSemanticsAction
import androidx.test.espresso.Espresso.pressBack
import io.github.naighes.scout4all.core.AnalysisPeriod
import io.github.naighes.scout4all.core.DetailFilter
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.EventTypeEnum
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import io.github.naighes.scout4all.core.Scouting
import io.github.naighes.scout4all.core.TeamSideEnum
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner

/** A touch of ours told event first: the event, the player, the evaluation. */
private fun touch(s: Scouting, event: EventTypeEnum, player: String, eval: EvalEnum) {
    s.chooseEvent(event)
    s.choosePlayer(player)
    s.chooseEval(eval)
}

/** The libero of `nationalTeam`, who never serves, and the setter, serving first. */
private const val LIBERO = "6 Monica De Gennaro"
private const val SETTER = "8 Alessia Orro"
private const val B_SERVER = "7 Myriam Sylla"

/** A match of the national team scouted with the core: Orro's ace; her
 * serve `+`, the libero's dig, their error; her serve error; the libero's
 * reception `#`, their error. The libero only receives and digs. */
private fun liberoMatch(team: String, match: String = startFirstSet(team)) = core { c ->
    c.scouting(team, match).use { s ->
        val libero = s.state().court.first { it.libero }.id
        s.chooseEval(EvalEnum.PERFECT)
        s.chooseEval(EvalEnum.POSITIVE)
        touch(s, EventTypeEnum.D, libero, EvalEnum.POSITIVE)
        s.chooseEvent(EventTypeEnum.OE)
        s.chooseEval(EvalEnum.ERROR)
        touch(s, EventTypeEnum.P, libero, EvalEnum.PERFECT)
        s.chooseEvent(EventTypeEnum.OE)
    }
}

private fun ComposeContentTestRule.click(text: String) {
    onNode(hasText(text) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
    waitForIdle()
}

/** An item of the menu open. */
private fun ComposeContentTestRule.choose(text: String) {
    onNode(hasText(text) and hasClickAction() and hasAnyAncestor(isPopup())).performSemanticsAction(SemanticsActions.OnClick)
    waitForIdle()
}

/** Until no menu is open. */
private fun ComposeContentTestRule.waitForNoMenu() = waitFor { onAllNodes(isPopup()).fetchSemanticsNodes().isEmpty() }

/** A player chosen in the menu of the filter, the menu closed. */
private fun ComposeContentTestRule.choosePlayer(chip: String, player: String) {
    click(chip)
    choose(player)
    pressBack()
    waitForNoMenu()
}

/** The analysis of the team open on its last matches. */
private fun ComposeContentTestRule.analysis(team: String, matches: Int = 1) {
    setContent { Scout4AllTheme { AnalysisScreen(team, onBack = {}) } }
    waitForText(string(R.string.last_matches, 5))
    click(string(R.string.last_matches, 5))
    waitForText(string(R.string.matches_count, matches), substring = true)
}

/** The players of `nationalTeam` the filter of the players offers, by
 * number: the menu opened, then closed choosing all of them. */
private fun ComposeContentTestRule.offered(): List<String> {
    click(string(R.string.filter_player))
    waitFor { onAllNodes(isPopup()).fetchSemanticsNodes().isNotEmpty() }
    val shown = nationalTeam.sortedBy { it.third }.map { (name, _, number) -> "$number $name" }
        .filter { onAllNodes(hasText(it) and hasAnyAncestor(isPopup())).fetchSemanticsNodes().isNotEmpty() }
    choose(string(R.string.filter_all))
    waitForNoMenu()
    return shown
}

/** The name of a fundamental on its chip in the detail, as the core names it. */
private fun fundamental(team: String, event: EventTypeEnum): String = core {
    it.teamAnalysis(team, AnalysisPeriod.LastMatches(5u), null, EventTypeEnum.S, DetailFilter(null))
}.detail.fundamentals.first { it.value == event }.name.replaceFirstChar { it.uppercase() }

/** The filter of the players of the analysis lists the libero, who never
 * serves, on every page but the details of the fundamentals they don't
 * play; with no hidden dependence on the fundamental of the detail chosen
 * before (TODO/000). In every language. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class AnalysisPlayersTest(language: String) {
    @get:Rule(order = 0) val size = SizeRule(Size(language, "$language-w392dp-h800dp-port"))
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun languages() = listOf("en", "it", "pt", "pl", "ja").map { arrayOf(it) }
    }

    @Test
    fun theLiberoIsOfferedOnEveryPageButTheDetailsWithoutThem() {
        val team = saveNationalTeam()
        liberoMatch(team)
        rule.analysis(team)
        val everyone = listOf(LIBERO, SETTER)
        // never through the detail: the libero is there already
        assertEquals(everyone, rule.offered())
        rule.click(string(R.string.stats_trend))
        assertEquals(everyone, rule.offered())
        rule.click(string(R.string.stats_detail))
        // the serve: only the setter served
        assertEquals(listOf(SETTER), rule.offered())
        rule.click(fundamental(team, EventTypeEnum.P))
        assertEquals(listOf(LIBERO), rule.offered())
        rule.click(fundamental(team, EventTypeEnum.D))
        assertEquals(listOf(LIBERO), rule.offered())
        rule.click(fundamental(team, EventTypeEnum.B))
        assertEquals(emptyList<String>(), rule.offered())
        // the fundamental of the detail changes nothing on the other pages
        rule.click(string(R.string.stats_summary))
        assertEquals(everyone, rule.offered())
        rule.click(string(R.string.stats_trend))
        assertEquals(everyone, rule.offered())
    }

    /** The libero chosen on the summary and on the trend: the figures
     * change, the chip names them; all of them again, as before. */
    @Test
    fun theLiberoChosenNarrowsTheFigures() {
        val team = saveNationalTeam()
        liberoMatch(team)
        rule.analysis(team)
        listOf(R.string.stats_summary, R.string.stats_trend).forEach { page ->
            rule.click(string(page))
            val before = rule.texts()
            rule.choosePlayer(string(R.string.filter_player), LIBERO)
            rule.waitForText(string(R.string.filter_player) + " 6")
            assertNotEquals(before, rule.texts())
            rule.click(string(R.string.filter_player) + " 6")
            rule.choose(string(R.string.filter_all))
            rule.waitForNoMenu()
            rule.waitFor({ "${rule.texts()} instead of $before" }) { rule.texts() == before }
        }
    }
}

/** Seen from another team, or two teams side by side: the players chosen
 * are of one team only. */
@RunWith(org.robolectric.RobolectricTestRunner::class)
class AnalysisPlayersOfTwoTeamsTest {
    @get:Rule val rule = createComposeRule()

    /** Seen from the opponent, the players chosen of the team are taken
     * off: none is chosen that the filter doesn't list. Comparing the two
     * teams, no filter of the players (as for a match). */
    /** Our ace (the setter's), our serve error, B's ace (B turned: of the
     * outside hitter in 2): one player of each team with a touch. */
    private fun bothServe(): String {
        val both = saveNationalTeam()
        serve(both, startFirstSetBoth(both), EvalEnum.PERFECT, EvalEnum.ERROR, EvalEnum.PERFECT)
        return both
    }

    private val chip get() = string(R.string.filter_player)

    /** Until the chip of the filter has nothing chosen. */
    private fun waitForNothingChosen() =
        rule.waitFor({ rule.texts().toString() }) {
            rule.onAllNodesWithText(chip).fetchSemanticsNodes().size == 1 &&
                rule.onAllNodesWithText(chip, substring = true).fetchSemanticsNodes().size == 1
        }

    @Test
    fun anotherTeamTakesThePlayersChosenOff() {
        val both = bothServe()
        rule.analysis(both)
        rule.choosePlayer(chip, SETTER)
        rule.waitForText("$chip 8")
        rule.click("Arezzo")
        waitForNothingChosen()
        // B's server, then the team again: nothing chosen
        rule.choosePlayer(chip, B_SERVER)
        rule.waitForText("$chip 7")
        rule.click(core { it.team(both).name })
        waitForNothingChosen()
    }

    /** Side by side, no filter of the players, the ones chosen taken off:
     * the same figures as with none chosen. */
    @Test
    fun comparingTakesThePlayersChosenOff() {
        val both = bothServe()
        rule.analysis(both)
        rule.click("Arezzo")
        rule.waitForText(string(R.string.compare))
        rule.click(string(R.string.compare))
        rule.waitForText(string(R.string.stats_errors))
        rule.waitFor({ rule.texts().toString() }) { rule.onAllNodesWithText(chip, substring = true).fetchSemanticsNodes().isEmpty() }
        val compared = rule.texts()
        // B's server chosen, then side by side again
        rule.click("Arezzo")
        waitForNothingChosen()
        rule.choosePlayer(chip, B_SERVER)
        rule.waitForText("$chip 7")
        rule.click(string(R.string.compare))
        rule.waitFor({ "${rule.texts()} instead of $compared" }) { rule.texts() == compared }
        // another page: the filter again, nothing chosen
        rule.click(string(R.string.stats_trend))
        waitForNothingChosen()
    }
}

/** Fourteen players with a touch, one of them with a long name: each one
 * can be reached in the menu of the filter, on every screen. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class AnalysisPlayersLayoutTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }

        /** In the order of `nationalTeam`: setter, outside hitter, middle
         * blocker, opposite, outside hitter, middle blocker, libero. */
        private val others = listOf(
            Triple("Maria Antonietta Benedetta Fontanarosa Lucchesini", RoleEnum.SETTER, 1),
            Triple("Carlotta Cambi", RoleEnum.OUTSIDE_HITTER, 2),
            Triple("Marina Lubian", RoleEnum.MIDDLE_BLOCKER, 3),
            Triple("Alessia Gennari", RoleEnum.OPPOSITE_HITTER, 10),
            Triple("Gaia Giovannini", RoleEnum.OUTSIDE_HITTER, 5),
            Triple("Loveth Omoruyi", RoleEnum.MIDDLE_BLOCKER, 12),
            Triple("Ilaria Spirito", RoleEnum.LIBERO, 4),
        )
    }

    @Test
    fun everyPlayerCanBeReachedInTheMenu() {
        val team = saveNationalTeam()
        others.forEach { (name, role, number) -> core { it.savePlayer(team, null, PlayerForm(name, role, number.toString())) } }
        // each team twice, the lineup turned by three: the middle blocker
        // the libero is on court for in the back row is in the front row
        val lineups = listOf(nationalTeam to 0, nationalTeam to 3, others to 0, others to 3)
        lineups.forEachIndexed { i, (players, turn) ->
            core { c ->
                val match = c.saveMatch(team, null, MatchForm("Arezzo $i", "2026-10-2${i + 1}T18:00:00+02:00", true, "6"))
                val ids = c.team(team).players.associateBy { it.name }
                val start = c.setStart(team, match)
                start.chooseServingTeam(TeamSideEnum.US)
                players.take(6).forEachIndexed { at, (name, _, _) -> start.place(((at + turn) % 6 + 1).toUByte(), ids.getValue(name).id) }
                start.chooseSetter(ids.getValue(players[0].first).id)
                start.chooseLibero(ids.getValue(players[6].first).id)
                c.startSet(start)
                // our serve `+`, a dig of each one on court, their error
                c.scouting(team, match).use { s ->
                    s.state().court.forEach { p ->
                        s.chooseEval(EvalEnum.POSITIVE)
                        touch(s, EventTypeEnum.D, p.id, EvalEnum.POSITIVE)
                        s.chooseEvent(EventTypeEnum.OE)
                    }
                }
            }
        }
        rule.analysis(team, matches = lineups.size)
        rule.click(string(R.string.filter_player))
        rule.waitForText(string(R.string.filter_all))
        val everyone = (nationalTeam + others).sortedBy { it.third }.map { (name, _, number) -> "$number $name" }
        assertEquals(14, everyone.size)
        everyone.forEach { text ->
            val item = rule.onNode(hasText(text) and hasClickAction())
            item.performScrollTo()
            item.assertIsDisplayed()
        }
        rule.shot("analysis-players-$screen")
    }
}
