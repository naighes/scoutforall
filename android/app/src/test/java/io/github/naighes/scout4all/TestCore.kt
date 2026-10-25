package io.github.naighes.scout4all

import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.test.core.app.ApplicationProvider
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.GenderEnum
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import io.github.naighes.scout4all.core.Scout4All
import io.github.naighes.scout4all.core.TeamClassificationEnum
import io.github.naighes.scout4all.core.TeamForm
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
    it.saveTeam(null, TeamForm(name, TeamClassificationEnum.REGIONAL, GenderEnum.WOMEN, year.toString()))
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
 * `nationalTeam` in order; the match id. */
fun startFirstSet(team: String, opponent: String = "Cortona", serving: TeamSideEnum = TeamSideEnum.US): String = core { c ->
    val match = c.saveMatch(team, null, MatchForm(opponent, "2026-10-25T18:00:00+02:00", true, "6"))
    val start = c.setStart(team, match)
    start.chooseServingTeam(serving)
    val players = c.team(team).players.associateBy { it.name }
    nationalTeam.take(6).forEachIndexed { i, (name, _, _) ->
        start.place((i + 1).toUByte(), players.getValue(name).id)
    }
    start.chooseSetter(players.getValue("Alessia Orro").id)
    start.chooseLibero(players.getValue("Monica De Gennaro").id)
    c.startSet(start)
    match
}

/** The screens read the core off the main thread, which the rule doesn't
 * wait for: until the text is there. */
fun ComposeContentTestRule.waitForText(text: String, substring: Boolean = false) =
    waitUntil(timeoutMillis = 5_000) {
        onAllNodesWithText(text, substring = substring).fetchSemanticsNodes().isNotEmpty()
    }

/** Until `condition`, letting the main thread run: what the core answers
 * off it reaches the screen there. */
fun ComposeContentTestRule.waitFor(condition: () -> Boolean) {
    val end = System.currentTimeMillis() + 5_000
    while (true) {
        waitForIdle()
        if (condition()) return
        check(System.currentTimeMillis() < end) { "still not so after 5 s" }
        Thread.sleep(10)
    }
}

/** Our serves in the set in progress, scouted with the core: each
 * evaluation an event. */
fun serve(team: String, match: String, vararg evals: EvalEnum) = core { c ->
    c.scouting(team, match).use { scouting -> evals.forEach { scouting.chooseEval(it) } }
}
