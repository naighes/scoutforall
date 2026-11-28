package io.github.naighes.scout4all

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.junit4.ComposeTestRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.test.core.app.ApplicationProvider
import io.github.naighes.scout4all.core.DvwChoice
import io.github.naighes.scout4all.core.DvwOpponent
import io.github.naighes.scout4all.core.DvwSide
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.GenderEnum
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import io.github.naighes.scout4all.core.Scout4All
import io.github.naighes.scout4all.core.TeamClassificationEnum
import io.github.naighes.scout4all.core.TeamForm
import io.github.naighes.scout4all.core.TeamListing
import io.github.naighes.scout4all.core.TeamSideEnum
import kotlinx.coroutines.runBlocking

/** The app of the test: Robolectric gives each test a new one, with empty
 * data. */
val app: Scout4AllApplication get() = ApplicationProvider.getApplicationContext()

/** Calls the real core of the app, as the screens do. */
fun <T> core(call: (Scout4All) -> T): T = runBlocking { app.core(call) }

fun string(id: Int, vararg args: Any): String = app.getString(id, *args)

/** A team saved by the core; its id. */
fun saveTeam(name: String, year: Int = 2026): String = core {
    it.saveTeam(null, TeamForm(name, TeamClassificationEnum.REGIONAL, GenderEnum.WOMEN, "IT", year.toString()))
}

/** The players of `team`, as the national team: names of every length. */
val nationalTeam = listOf(
    Triple("Alessia Orro", RoleEnum.SETTER, 8),
    Triple("Myriam Sylla", RoleEnum.OUTSIDE_HITTER, 7),
    Triple("Sarah Fahr", RoleEnum.MIDDLE_BLOCKER, 17),
    Triple("Paola Egonu", RoleEnum.OPPOSITE_HITTER, 18),
    Triple("Ekaterina Antropova", RoleEnum.OUTSIDE_HITTER, 9),
    Triple("Anna Danesi", RoleEnum.MIDDLE_BLOCKER, 11),
    Triple("Monica De Gennaro", RoleEnum.LIBERO, 6),
)

/** A team with the players of `nationalTeam`; its id. */
fun saveNationalTeam(): String {
    val team = saveTeam("Italia")
    nationalTeam.forEach { (name, role, number) ->
        core { it.savePlayer(team, null, PlayerForm(name, role, number.toString())) }
    }
    return team
}

/** A match of the team, with its first set started: the players of
 * `nationalTeam` in order, the setter first, in position `setterAt`; the
 * match id. */
fun startFirstSet(
    team: String,
    opponent: String = "Cortona",
    serving: TeamSideEnum = TeamSideEnum.US,
    setterAt: Int = 1,
    competition: String = "",
): String = core { c ->
    val match = c.saveMatch(team, null, MatchForm(opponent, "2026-10-25T18:00:00+02:00", true, "6", competition = competition))
    startSet(c, team, match, serving, setterAt)
    match
}

/** A match on both sides with its first set started: Arezzo, with the
 * players of `nationalTeam` too, A serving; the match id. */
fun startFirstSetBoth(team: String, opponent: String = "Arezzo"): String = core { c ->
    val b = saveTeam(opponent)
    nationalTeam.forEach { (name, role, number) ->
        c.savePlayer(b, null, PlayerForm(name, role, number.toString()))
    }
    val match = c.saveMatch(team, null, MatchForm(opponent, "2026-10-25T18:00:00+02:00", true, "6", opponentTeam = b, both = true))
    c.saveRosterOfB(team, match, MatchForm("", null, false, ""))
    val lineup = { start: io.github.naighes.scout4all.core.SetStart, id: String ->
        val players = c.team(id).players.associateBy { it.name }
        nationalTeam.take(6).forEachIndexed { i, (name, _, _) -> start.place((i + 1).toUByte(), players.getValue(name).id) }
        start.chooseSetter(players.getValue("Alessia Orro").id)
        start.chooseLibero(players.getValue("Monica De Gennaro").id)
    }
    val a = c.setStart(team, match)
    a.chooseServingTeam(TeamSideEnum.US)
    lineup(a, team)
    val ofB = checkNotNull(c.setStartOfB(team, match))
    lineup(ofB, b)
    c.startSetBoth(a, ofB)
    match
}

/** A match of the team won 3–0 with aces; its id. */
fun winMatch(team: String, opponent: String, competition: String = ""): String {
    val match = startFirstSet(team, opponent, competition = competition)
    repeat(3) { set ->
        if (set > 0) startNextSet(team, match)
        core { c -> c.scouting(team, match).use { s -> repeat(25) { s.chooseEval(EvalEnum.PERFECT) } } }
    }
    return match
}

/** The next set of the match started, with the lineup of `startFirstSet`;
 * who serves as the rules say, or `serving` in the tie-break. */
fun startNextSet(team: String, match: String, serving: TeamSideEnum = TeamSideEnum.US) =
    core { startSet(it, team, match, serving, setterAt = 1) }

private fun startSet(c: Scout4All, team: String, match: String, serving: TeamSideEnum, setterAt: Int) {
    val start = c.setStart(team, match)
    start.chooseServingTeam(serving)
    val players = c.team(team).players.associateBy { it.name }
    nationalTeam.take(6).forEachIndexed { i, (name, _, _) ->
        start.place(((i + setterAt - 1) % 6 + 1).toUByte(), players.getValue(name).id)
    }
    start.chooseSetter(players.getValue("Alessia Orro").id)
    start.chooseLibero(players.getValue("Monica De Gennaro").id)
    c.startSet(start)
}

/** Until the text is there; if it never comes, what's there instead. */
fun ComposeTestRule.waitForText(text: String, substring: Boolean = false) =
    waitFor({ "no \"$text\" in ${texts()}" }) {
        onAllNodesWithText(text, substring = substring).fetchSemanticsNodes().isNotEmpty()
    }

/** Until the scouting waits for a player on the court, or an event of the
 * rally: the points without a cause are there (`+1`, the team in its description). */
fun ComposeTestRule.waitForTheRally() = waitFor({ "no \"+1\" in ${texts()}" }) {
    onAllNodesWithText("+1").fetchSemanticsNodes().isNotEmpty()
}

/** The texts on the screen, to tell what's there instead. */
fun ComposeTestRule.texts(): List<String> =
    onAllNodes(SemanticsMatcher("any") { true }).fetchSemanticsNodes()
        .mapNotNull { it.config.getOrNull(SemanticsProperties.Text)?.joinToString(" ") }

/** How long a test waits for the screen before failing: only a failing test
 * waits it all. Wide on purpose: the first test of a run is cold (the
 * native core loads, the first screen composes) and takes seconds. */
const val WAIT_MS = 30_000L

/** Until `condition`, letting the main thread run: what the core answers
 * off it reaches the screen there. */
fun ComposeTestRule.waitFor(why: () -> String = { "" }, condition: () -> Boolean) {
    val end = System.nanoTime() + WAIT_MS * 1_000_000
    while (true) {
        waitForIdle()
        if (condition()) return
        check(System.nanoTime() < end) { "still not so after ${WAIT_MS / 1000} s ${why()}" }
        Thread.sleep(10)
    }
}

/** Our serves in the set in progress, scouted with the core: each
 * evaluation an event. */
fun serve(team: String, match: String, vararg evals: EvalEnum) = core { c ->
    c.scouting(team, match).use { scouting -> evals.forEach { scouting.chooseEval(it) } }
}

/** "Home"–"Visiting" of the TUI fixture, imported into "Home" on both
 * sides, "Visiting" a new team: the match of A and the team B. */
fun bothSides(): Triple<String, String, String> {
    val home = saveTeam("Home")
    val bytes = app.javaClass.classLoader!!.getResourceAsStream("three_sets.dvw")!!.use { it.readBytes() }
    val choice = DvwChoice(
        side = DvwSide.HOME,
        day = "2015-01-25",
        maxSubstitutions = 6u,
        teamId = home,
        classification = null,
        gender = null,
        country = null,
        opponent = DvwOpponent.NewTeam,
    )
    val m = core { it.importDvw(bytes, choice) }.matchId
    val b = core { it.teams() }.firstNotNullOf { t -> (t as? TeamListing.Team)?.takeIf { it.name == "Visiting" }?.id }
    return Triple(home, m, b)
}
