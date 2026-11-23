package io.github.naighes.scout4all

import androidx.compose.runtime.Composable
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasScrollToNodeAction
import androidx.compose.ui.test.performScrollToNode
import androidx.compose.ui.test.hasAnyDescendant
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.semantics.SemanticsActions
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
    fun theHome() {
        val team = saveNationalTeam()
        startFirstSet(team, "Cortona Volley Pallavolo Femminile Valdichiana")
        winMatch(team, "Serbia")
        assertFits("home", string(R.string.resume), string(R.string.resume)) {
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
    fun theMatchesOfTheTeam() {
        val team = saveNationalTeam()
        startFirstSet(team)
        rule.setContent {
            Scout4AllTheme {
                TeamScreen(team, onBack = {}, onEdit = {}, onNewPlayer = {}, onPlayer = {}, onNewMatch = {}, onMatch = {})
            }
        }
        rule.waitForText("Ekaterina Antropova")
        rule.onNode(hasText(string(R.string.matches)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        // today, always in the week, may push the match below a short screen
        rule.waitForText(string(R.string.tr_programming))
        rule.onNode(hasScrollToNodeAction()).performScrollToNode(hasText("Cortona"))
        rule.waitForText("Cortona")
        rule.shot("screen-team-matches-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
    }

    /** A training today: in the home and in the team, in the week (above
     * the matches in the team), in the block of today. */
    private fun planTraining(team: String): String {
        val start = java.time.LocalDate.now().toString() + "T18:00"
        core { it.saveSession(team, null, it.newSessionDraft().copy(start = start, duration = "90", title = "Ricezione")) }
        return "Ricezione"
    }

    @Test
    fun theAgendaOfTheHome() {
        val team = saveNationalTeam()
        val title = planTraining(team)
        assertFits("home-agenda", title) {
            TeamsScreen(TeamsViewModel(app), onTeam = {}, onNewTeam = {}, onSettings = {}, onMatch = { _, _ -> })
        }
        // no card of today on its own: today is lit in the week, the
        // training in it; "today" is only the button of the week
        rule.onAllNodes(hasText(string(R.string.agenda_today))).assertCountEquals(1)
        rule.onNode(hasTestTag(TODAY_TAG) and hasAnyDescendant(hasText(title))).assertExists()
    }

    @Test
    fun theWeekOfTheHomeShowsTodayWithNothingPlanned() {
        saveNationalTeam()
        rule.setContent {
            Scout4AllTheme {
                TeamsScreen(TeamsViewModel(app), onTeam = {}, onNewTeam = {}, onSettings = {}, onMatch = { _, _ -> })
            }
        }
        rule.waitUntil(10_000) { rule.onAllNodes(hasTestTag(TODAY_TAG)).fetchSemanticsNodes().isNotEmpty() }
        rule.onNode(hasTestTag(TODAY_TAG) and hasAnyDescendant(hasText(string(R.string.agenda_nothing)))).assertExists()
    }

    @Test
    fun theWeekOfTheTeam() {
        val team = saveNationalTeam()
        val title = planTraining(team)
        rule.setContent {
            Scout4AllTheme {
                TeamScreen(team, onBack = {}, onEdit = {}, onNewPlayer = {}, onPlayer = {}, onNewMatch = {}, onMatch = {})
            }
        }
        rule.waitForText("Ekaterina Antropova")
        rule.onNode(hasText(string(R.string.matches)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(title)
        rule.shot("screen-team-week-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
    }

    /** The training screens fit every screen: the sessions, a session with
     * its activities, the forms, the attendance. */
    private fun trainingWithActivities(team: String): String {
        val gym = core { it.createVenueNamed("Palestra comunale") }.id
        val session = core {
            it.saveSession(
                team, null,
                it.newSessionDraft().copy(
                    start = java.time.LocalDate.now().plusDays(1).toString() + "T18:00",
                    duration = "90", title = "Ricezione e attacco", venue = gym,
                    objectives = "Ricezione a tre, cambio palla", notes = "Portare i palloni leggeri",
                ),
            )
        }
        listOf("Palleggio a coppie", "Bagher contro il muro", "Partitella a tema con servizio float").forEach { name ->
            core { it.saveActivity(session.id, null, it.newActivityDraft().copy(name = name, planned = "20")) }
        }
        return session.id
    }

    @Test
    fun theSessionsOfTheTeam() {
        val team = saveNationalTeam()
        trainingWithActivities(team)
        assertFits("sessions", "Ricezione e attacco", null) {
            Hub(team, Part.SESSIONS)
        }
    }

    @Composable
    private fun Hub(team: String, part: Part) = TrainingScreen(
        team, onBack = {}, onSession = {}, onNew = {}, onSeries = {}, onCycle = { _, _ -> }, onNewCycle = {}, onNewOn = {},
        onMatch = {}, first = part,
    )

    private fun week(team: String): String = core {
        val monday = java.time.LocalDate.now().let { d -> d.minusDays((d.dayOfWeek.value - 1).toLong()) }
        it.saveCycle(
            team, io.github.naighes.scout4all.core.Level.MICROCYCLE, null,
            it.newCycleDraft().copy(title = "Settimana di ricezione", start = monday.toString(), weeks = "1"),
            emptyList(), false,
        )
    }.id

    @Test
    fun theCalendarOfTheTeam() {
        val team = saveNationalTeam()
        planTraining(team)
        assertFits("calendar", string(R.string.agenda_today), null) { Hub(team, Part.CALENDAR) }
    }

    @Test
    fun theCyclesOfTheTeam() {
        val team = saveNationalTeam()
        week(team)
        assertFits("cycles", "Settimana di ricezione", null) { Hub(team, Part.CYCLES) }
    }

    @Test
    fun theAttendanceOfTheTeam() {
        val team = saveNationalTeam()
        assertFits("attendance-report", string(R.string.tr_no_attendance_to_count).replaceFirstChar { it.uppercase() }, null) {
            Hub(team, Part.ATTENDANCE)
        }
    }

    @Test
    fun aCycle() {
        val team = saveNationalTeam()
        val cycle = week(team)
        assertFits("cycle", string(R.string.tr_cycle_totals), string(R.string.tr_add_children)) {
            CycleScreen(
                team, io.github.naighes.scout4all.core.Level.MICROCYCLE, cycle,
                onBack = {}, onEdit = {}, onAdd = {}, onDuplicate = {}, onChild = {},
            )
        }
    }

    @Test
    fun theFormOfACycle() {
        val team = saveNationalTeam()
        assertFits("cycle-form", string(R.string.tr_cycle_weeks), string(R.string.save)) {
            CycleFormScreen(team, io.github.naighes.scout4all.core.Level.MICROCYCLE, null, onBack = {}, onSaved = {})
        }
    }

    @Test
    fun theCopyOfACycle() {
        val team = saveNationalTeam()
        val cycle = week(team)
        assertFits("cycle-duplicate", string(R.string.tr_copies), string(R.string.tr_duplicate_microcycle)) {
            DuplicateCycleScreen(cycle, onBack = {}, onDone = {})
        }
    }

    @Test
    fun aSession() {
        val team = saveNationalTeam()
        val session = trainingWithActivities(team)
        assertFits("session", string(R.string.tr_activities), string(R.string.tr_complete_session)) {
            SessionScreen(team, session, onBack = {}, onEdit = {}, onActivity = {}, onAttendance = {}, onDuplicate = {}, onSeries = {})
        }
    }

    @Test
    fun theFormOfASession() {
        val team = saveNationalTeam()
        assertFits("session-form", string(R.string.tr_session_duration), string(R.string.save)) {
            SessionFormScreen(team, null, onBack = {}, onSaved = {})
        }
    }

    @Test
    fun theFormOfAnActivity() {
        val team = saveNationalTeam()
        val session = trainingWithActivities(team)
        assertFits("activity-form", string(R.string.tr_copy_activity), string(R.string.save)) {
            ActivityFormScreen(team, session, null, onBack = {}, onDone = {})
        }
    }

    @Test
    fun theAttendanceOfASession() {
        val team = saveNationalTeam()
        val session = trainingWithActivities(team)
        assertFits("attendance", "Alessia Orro", string(R.string.save)) {
            AttendanceScreen(team, session, onBack = {})
        }
    }

    @Test
    fun theCopyOfASession() {
        val team = saveNationalTeam()
        val session = trainingWithActivities(team)
        assertFits("duplicate", string(R.string.tr_copy_objectives), string(R.string.tr_duplicate_session)) {
            DuplicateScreen(team, session, onBack = {}, onDuplicated = {})
        }
    }

    @Test
    fun theChangeOfASeries() {
        val team = saveNationalTeam()
        val first = java.time.LocalDate.now().plusDays(1)
        val made = core {
            it.createSeries(
                team,
                io.github.naighes.scout4all.core.SeriesDraft(
                    first.toString(), first.plusWeeks(3).toString(),
                    listOf(io.github.naighes.scout4all.core.SeriesBand(first.dayOfWeek.value.toUByte(), "20:30", 90.toUShort(), null)),
                ),
                false,
            )
        }
        val session = made.created.first().id
        assertFits("series-change", string(R.string.tr_series_scope), string(R.string.save)) {
            SeriesChangeScreen(team, session, onBack = {}, onDone = {})
        }
    }

    @Test
    fun theFormOfASeries() {
        val team = saveNationalTeam()
        assertFits("series", string(R.string.tr_band_hint), string(R.string.save)) {
            SeriesScreen(team, onBack = {}, onDone = {})
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
        // the statistics a button of their own, whole under the score
        rule.assertWhole(hasText(string(R.string.match_stats)) and hasClickAction())
    }

    /** A match of the team as B: the result, why only that, and the button
     * to open it as A, whole. */
    @Test
    fun theMatchSeenByB() {
        val team = saveNationalTeam()
        val serbia = saveTeam("Serbia")
        val match = core {
            it.saveMatch(team, null, MatchForm("Serbia", "2026-10-25T18:00:00+02:00", true, "6", opponentTeam = serbia))
        }
        startNextSet(team, match)
        serve(team, match, EvalEnum.PERFECT)
        val open = string(R.string.open_as_team, "Italia")
        assertFits("match-b", open, open) {
            MatchScreen(serbia, match, onBack = {}, onEdit = {}, onStartSet = {}, onResumeSet = {}, onSet = {}, onStats = {})
        }
    }

    @Test
    fun theCompetitions() {
        val team = saveNationalTeam()
        listOf("Serie A1 femminile – stagione regolare", "Coppa Italia", "CEV Champions League").forEach { name ->
            core { it.saveMatch(team, null, MatchForm("Cortona", "2026-10-25T18:00:00+02:00", true, "6", competition = name)) }
        }
        assertFits("competitions", "Coppa Italia") { CompetitionsScreen(onBack = {}, onCompetition = {}) }
    }

    /** The players of the new season as tiles, the button to save whole. */
    @Test
    fun theNewSeasonPlayers() {
        val team = saveNationalTeam()
        rule.setContent { Scout4AllTheme { NewSeasonScreen(team, onBack = {}, onSaved = {}) } }
        rule.waitForText(string(R.string.new_season))
        rule.onNode(hasText(string(R.string.go_on)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.season_count, 7))
        rule.shot("screen-season-players-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
        rule.assertWhole(hasText(string(R.string.save)) and hasClickAction())
    }

    @Test
    fun theMatchWithLongNotes() {
        val team = saveNationalTeam()
        val match = startFirstSet(team)
        serve(team, match, EvalEnum.PERFECT)
        val form = core { it.matchForm(team, match) }
        core { it.saveMatch(team, match, form.copy(notes = "Palestra fredda, rete bassa. ".repeat(20))) }
        // the notes whole, after the sets: the statistics and the set still in sight
        assertFits("match-notes", string(R.string.match_stats), string(R.string.resume_set_n, 1)) {
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
