package io.github.naighes.scout4all

import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasAnyAncestor
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.isDialog
import androidx.compose.ui.test.performTextReplacement
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.hasTestTag
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.click
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performScrollToNode
import io.github.naighes.scout4all.core.ActivityStatus
import io.github.naighes.scout4all.core.AttendanceStatus
import io.github.naighes.scout4all.core.Level
import io.github.naighes.scout4all.core.SessionStatus
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onAllNodesWithContentDescription
import androidx.compose.ui.test.onLast
import android.content.Intent
import android.net.Uri
import androidx.test.core.app.ApplicationProvider
import io.github.naighes.scout4all.core.ArchivePreview
import org.junit.Assert.assertEquals
import org.robolectric.Shadows.shadowOf
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.time.LocalDate
import java.time.LocalTime

/** The training on the phone: a session written in its form, its activities
 * and state, its attendance, a copy of it. */
@RunWith(RobolectricTestRunner::class)
class TrainingScreensTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()

    private fun session(day: LocalDate = LocalDate.now().plusDays(1), title: String = "Ricezione"): String =
        core {
            it.saveSession(team, null, it.newSessionDraft().copy(start = "${day}T18:00", duration = "90", title = title))
        }.id

    private fun activity(session: String, name: String) =
        core { it.saveActivity(session, null, it.newActivityDraft().copy(name = name, planned = "20")) }.id

    private fun show(session: String) {
        rule.setContent {
            Scout4AllTheme {
                SessionScreen(
                    team, session, onBack = {}, onEdit = {}, onActivity = {}, onAttendance = {}, onDuplicate = {},
                )
            }
        }
        rule.waitForText("Ricezione")
    }

    private fun press(text: String) {
        rule.onNode(hasText(text) and hasClickAction()).performClick()
        rule.waitForIdle()
    }

    @Test
    fun aSessionIsWrittenInItsForm() {
        val saved = mutableListOf<String>()
        rule.setContent { Scout4AllTheme { SessionFormScreen(team, null, onBack = {}, onSaved = { saved += it }) } }
        rule.waitForText(string(R.string.tr_session_duration))
        rule.type(string(R.string.tr_session_duration), "75")
        rule.type(string(R.string.tr_session_title), "Battuta")
        rule.save()
        rule.waitFor({ "not saved" }) { saved.isNotEmpty() }
        val sessions = core { it.trainingSessions(team) }
        assertEquals(listOf("Battuta"), sessions.map { it.title })
        assertEquals(75.toUShort(), sessions.single().durationMin)
        assertEquals(saved, sessions.map { it.id })
    }

    @Test
    fun aDurationThatIsNotANumberIsShownOnItsField() {
        val saved = mutableListOf<String>()
        rule.setContent { Scout4AllTheme { SessionFormScreen(team, null, onBack = {}, onSaved = { saved += it }) } }
        rule.waitForText(string(R.string.tr_session_duration))
        rule.type(string(R.string.tr_session_duration), "ninety")
        rule.save()
        rule.assertInvalid(string(R.string.tr_session_duration))
        assertEquals(emptyList<String>(), saved)
        assertEquals(0, core { it.trainingSessions(team) }.size)
    }

    @Test
    fun anActivityMovesDown() {
        val s = session()
        val first = activity(s, "Palleggio")
        activity(s, "Bagher")
        show(s)
        rule.onAllNodes(hasContentDescription(string(R.string.tr_move_down)) and hasClickAction()).onFirst().performClick()
        rule.waitFor({ "not moved" }) { core { it.sessionActivities(s) }.last().id == first }
    }

    @Test
    fun completingAsksForThePlannedActivities() {
        val s = session()
        val kept = activity(s, "Palleggio")
        val left = activity(s, "Bagher")
        show(s)
        press(string(R.string.tr_complete_session))
        // the dialog: one activity is left planned
        rule.waitForText(string(R.string.tr_complete_activities))
        rule.onNode(hasText("Bagher") and isToggleable()).performClick()
        // the button of the dialog is the last one
        rule.onAllNodes(hasText(string(R.string.tr_complete_session)) and hasClickAction()).let {
            it[it.fetchSemanticsNodes().size - 1].performClick()
        }
        rule.waitFor({ "not completed" }) { core { it.trainingSession(s) }.status == SessionStatus.COMPLETED }
        val status = core { it.sessionActivities(s) }.associate { it.id to it.status }
        assertEquals(ActivityStatus.COMPLETED, status[kept])
        assertEquals(ActivityStatus.PLANNED, status[left])
    }

    @Test
    fun aCancelledSessionIsReopened() {
        val s = session()
        show(s)
        press(string(R.string.tr_cancel_session))
        rule.waitFor({ "not cancelled" }) { core { it.trainingSession(s) }.status == SessionStatus.CANCELLED }
        press(string(R.string.tr_reopen_session))
        rule.waitFor({ "not reopened" }) { core { it.trainingSession(s) }.status == SessionStatus.PLANNED }
    }

    @Test
    fun aSessionIsDuplicatedAWeekLater() {
        val day = LocalDate.now().plusDays(1)
        val s = session(day)
        activity(s, "Palleggio")
        val copies = mutableListOf<String>()
        rule.setContent { Scout4AllTheme { DuplicateScreen(team, s, onBack = {}, onDuplicated = { copies += it }) } }
        rule.waitForText(string(R.string.tr_copy_objectives))
        press(string(R.string.tr_duplicate_session))
        rule.waitFor({ "not duplicated" }) { copies.isNotEmpty() }
        val copy = core { it.trainingSession(copies.single()) }
        assertEquals("${day.plusWeeks(1)}T18:00", copy.start)
        assertEquals(listOf("Palleggio"), core { it.sessionActivities(copy.id) }.map { it.name })
    }

    @Test
    fun theAttendanceIsSavedAsTouched() {
        val s = session()
        val back = mutableListOf<Unit>()
        rule.setContent { Scout4AllTheme { AttendanceScreen(team, s, onBack = { back += Unit }) } }
        rule.waitForText("Alessia Orro")
        // nothing written before the first save
        assertTrue(!core { it.sessionAttendance(s) }.recorded)
        val first = core { it.sessionAttendance(s) }.rows.first()
        // a touch on the state of the first row: present, then late
        rule.onAllNodes(hasText(string(R.string.tr_present)) and hasClickAction()).onFirst().performClick()
        rule.waitForText(string(R.string.tr_late))
        rule.save()
        rule.waitFor({ "not saved" }) { back.isNotEmpty() }
        val saved = core { it.sessionAttendance(s) }
        assertTrue(saved.recorded)
        assertEquals(AttendanceStatus.LATE, saved.rows.single { it.personId == first.personId }.status)
        assertEquals(saved.rows.size - 1, saved.rows.count { it.status == AttendanceStatus.PRESENT })
    }

    @Test
    fun aNewPersonIsAGuestAndMakesUpFromTheNote() {
        val s = session()
        val back = mutableListOf<Unit>()
        rule.setContent { Scout4AllTheme { AttendanceScreen(team, s, onBack = { back += Unit }) } }
        rule.waitForText("Alessia Orro")
        rule.onNodeWithContentDescription(string(R.string.tr_add_guest)).performClick()
        rule.onNode(hasSetTextAction() and hasText(string(R.string.tr_search))).performTextReplacement("Zoe Esterna")
        press(string(R.string.tr_new_person, "Zoe Esterna"))
        rule.waitForText("Zoe Esterna")
        // a person in no team doesn't make up by default: the note ticks it
        rule.onAllNodesWithContentDescription(string(R.string.tr_attendance_note)).onLast().performClick()
        press(string(R.string.tr_makeup))
        press(string(R.string.ok))
        rule.waitForText(string(R.string.tr_guest) + " · " + string(R.string.tr_makeup))
        rule.save()
        rule.waitFor({ "not saved" }) { back.isNotEmpty() }
        val zoe = core { it.sessionAttendance(s) }.rows.single { it.name == "Zoe Esterna" }
        assertTrue(zoe.guest && zoe.makeup)
    }

    private fun hub(
        part: Part,
        onNewOn: (LocalDate) -> Unit = {},
        onNewAt: (LocalDate, LocalTime) -> Unit = { _, _ -> },
        onSession: (String) -> Unit = {},
        onNewCycle: (Level) -> Unit = {},
        onCreateFrom: (Level, List<String>) -> Unit = { _, _ -> },
        onNewMatch: (LocalDate) -> Unit = {},
    ) = rule.setContent {
        Scout4AllTheme {
            TrainingScreen(
                team, onBack = {}, onSession = onSession, onCycle = { _, _ -> },
                onNewCycle = onNewCycle, onNewOn = onNewOn, onNewAt = onNewAt, onMatch = {}, first = part,
                onCreateFrom = onCreateFrom, onNewMatch = onNewMatch,
            )
        }
    }

    @Test
    fun aMicrocycleIsWrittenInItsForm() {
        val saved = mutableListOf<String>()
        rule.setContent { Scout4AllTheme { CycleFormScreen(team, Level.MICROCYCLE, null, onBack = {}, onSaved = { saved += it }) } }
        rule.waitForText(string(R.string.tr_cycle_weeks))
        rule.type(string(R.string.tr_session_title), "Carico")
        rule.save()
        rule.waitFor({ "not saved" }) { saved.isNotEmpty() }
        val cycles = core { it.cycles(team, Level.MICROCYCLE) }
        assertEquals(listOf("Carico"), cycles.map { it.title })
        assertEquals(saved, cycles.map { it.id })
    }

    @Test
    fun theSessionsOfAWeekAreAddedToItsMicrocycle() {
        val monday = LocalDate.now().let { it.minusDays((it.dayOfWeek.value - 1).toLong()) }
        val cycle = core {
            it.saveCycle(team, Level.MICROCYCLE, null, it.newCycleDraft().copy(title = "Carico", start = monday.toString(), weeks = "1"), emptyList(), false)
        }
        val s = core {
            it.saveSession(team, null, it.newSessionDraft().copy(start = "${monday.plusDays(2)}T18:00", duration = "90", title = "Ricezione"))
        }
        // inside the window but not attached: free
        val done = mutableListOf<Unit>()
        rule.setContent { Scout4AllTheme { AddChildrenScreen(Level.MICROCYCLE, cycle.id, onBack = {}, onDone = { done += Unit }) } }
        rule.waitForText(core { it.freeChildren(Level.MICROCYCLE, cycle.id) }.first { c -> c.id == s.id }.let { c -> windowText(c.firstDay, c.lastDay) })
        rule.onNode(isToggleable()).performClick()
        press(string(R.string.tr_add_children))
        rule.waitFor({ "not added" }) { done.isNotEmpty() }
        assertEquals(listOf(s.id), core { it.cycleChildren(Level.MICROCYCLE, cycle.id) }.map { c -> c.id })
    }

    /** The sheet of a microcycle lists its sessions under their day, as
     * the list of the calendar, not the free ones; it makes a session on
     * its first day and offers the free ones only when there are some. */
    @Test
    fun theSheetOfAMicrocycleListsItsSessionsAndMakesOne() {
        val monday = LocalDate.now().plusWeeks(2).let { it.minusDays((it.dayOfWeek.value - 1).toLong()) }
        val cycle = core {
            it.saveCycle(team, Level.MICROCYCLE, null, it.newCycleDraft().copy(title = "Carico", start = monday.toString(), weeks = "1"), emptyList(), false)
        }
        val held = session(monday.plusDays(1), "Ricezione")
        core { it.addChildren(Level.MICROCYCLE, cycle.id, listOf(held)) }
        val days = mutableListOf<LocalDate>()
        val opened = mutableListOf<String>()
        rule.setContent {
            Scout4AllTheme {
                CycleScreen(
                    team, Level.MICROCYCLE, cycle.id,
                    onBack = {}, onEdit = {}, onAdd = {}, onDuplicate = {},
                    onChild = { opened += it }, onNewChild = { days += it }, onMatch = {},
                )
            }
        }
        rule.waitForText("Ricezione")
        rule.onNodeWithText(fullDayText(monday.plusDays(1))).assertExists()
        // nothing free: nothing to add
        rule.onNodeWithText(string(R.string.tr_add_sessions)).assertDoesNotExist()
        rule.onNodeWithText("Ricezione").performClick()
        assertEquals(listOf(held), opened)
        press(string(R.string.tr_new_session))
        assertEquals(listOf(monday), days)
    }

    /** A free session in the window: the sheet offers to add it, and does
     * not list it. */
    @Test
    fun theSheetOfAMicrocycleOffersTheFreeOnesOnly() {
        val monday = LocalDate.now().plusWeeks(2).let { it.minusDays((it.dayOfWeek.value - 1).toLong()) }
        val cycle = core {
            it.saveCycle(team, Level.MICROCYCLE, null, it.newCycleDraft().copy(start = monday.toString(), weeks = "1"), emptyList(), false)
        }
        session(monday.plusDays(2), "Libera")
        rule.setContent {
            Scout4AllTheme {
                CycleScreen(
                    team, Level.MICROCYCLE, cycle.id,
                    onBack = {}, onEdit = {}, onAdd = {}, onDuplicate = {}, onChild = {}, onNewChild = {}, onMatch = {},
                )
            }
        }
        rule.waitForText(string(R.string.tr_add_sessions))
        rule.onNodeWithText(string(R.string.tr_no_sessions)).assertExists()
        rule.onNodeWithText("Libera").assertDoesNotExist()
    }

    @Test
    fun theCalendarOpensANewSessionOnTheDay() {
        val days = mutableListOf<LocalDate>()
        hub(Part.CALENDAR, onNewOn = { days += it })
        rule.waitForText(string(R.string.agenda_today))
        // the + says what it makes
        rule.onNode(hasContentDescription(string(R.string.tr_new_session)) and hasClickAction()).performClick()
        rule.onNodeWithText(string(R.string.tr_new_session), useUnmergedTree = true).assertExists()
        assertEquals(listOf(LocalDate.now()), days)
        // only there: the day chosen doesn't offer it again
        rule.onNodeWithText("Add a session").assertDoesNotExist() // the old link, gone
    }

    /** The day chosen in the month offers a match on it too. */
    @Test
    fun theCalendarOpensANewMatchOnTheDay() {
        val days = mutableListOf<LocalDate>()
        hub(Part.CALENDAR, onNewMatch = { days += it })
        rule.waitForText(string(R.string.agenda_today))
        rule.onNode(hasText(string(R.string.new_match)) and hasClickAction()).performScrollTo().performClick()
        assertEquals(listOf(LocalDate.now()), days)
    }

    /** The month fills its weeks with the days of the months around, in
     * grey, as on the desktop: as many as its first and last week leave. */
    @Test
    fun theMonthShowsTheDaysOfTheMonthsAround() {
        hub(Part.CALENDAR)
        rule.waitForText(string(R.string.agenda_today))
        val month = java.time.YearMonth.now()
        val lead = month.atDay(1).dayOfWeek.value - 1
        val cells = lead + month.lengthOfMonth()
        val around = (cells + 6) / 7 * 7 - month.lengthOfMonth()
        rule.waitForIdle()
        assertEquals(
            around,
            rule.onAllNodesWithTag(DAY_AROUND, useUnmergedTree = true).fetchSemanticsNodes().size,
        )
    }

    /** The day of a date picker written by hand (its text mode). */
    private fun pickDay(field: String, day: LocalDate) {
        rule.onNode(hasText(field)).performScrollTo().performClick()
        rule.waitForIdle()
        rule.onNode(hasContentDescription("text input mode", substring = true, ignoreCase = true) and hasClickAction())
            .performClick()
        rule.onNode(hasSetTextAction() and hasAnyAncestor(isDialog()))
            .performTextReplacement(day.format(java.time.format.DateTimeFormatter.ofPattern("MMddyyyy")))
        rule.onNode(hasText(string(R.string.ok)) and hasClickAction()).performClick()
        rule.waitForIdle()
    }

    private fun inDialog(text: String) {
        rule.onNode(hasText(text) and hasClickAction() and hasAnyAncestor(isDialog())).performClick()
        rule.waitForIdle()
    }

    /** A weekly series, entry by entry: Continue keeps the entry (the last
     * day wanted first) and offers the day after; Finish makes them all,
     * each entry a series of its own. */
    @Test
    fun aSeriesIsWrittenEntryByEntry() {
        val monday = LocalDate.now().plusWeeks(1).with(java.time.DayOfWeek.MONDAY)
        var repeated = false
        rule.setContent {
            Scout4AllTheme {
                SessionFormScreen(team, null, onBack = {}, onSaved = {}, day = monday, onRepeated = { repeated = true })
            }
        }
        rule.waitForText(string(R.string.tr_weekly_series))
        press(string(R.string.tr_weekly_series))
        press(string(R.string.tr_continue_entry))
        rule.waitForText(string(R.string.tr_entry_invalid))
        pickDay(string(R.string.tr_until), monday.plusWeeks(2))
        press(string(R.string.tr_continue_entry))
        rule.waitForText(string(R.string.tr_series_entry, 2), substring = true)
        press(string(R.string.tr_finish))
        rule.waitFor({ "not made" }) { repeated }
        val sessions = core { it.trainingSessions(team) }
        assertEquals(
            listOf(monday, monday.plusDays(1), monday.plusWeeks(1), monday.plusWeeks(1).plusDays(1), monday.plusWeeks(2)),
            sessions.map { LocalDate.parse(it.start.take(10)) },
        )
        assertEquals(2, sessions.mapNotNull { it.seriesId }.toSet().size)
        assertTrue(sessions.all { it.durationMin == 120.toUShort() })
    }

    private fun twoEntries(): List<io.github.naighes.scout4all.core.SessionRecord> {
        val monday = LocalDate.now().plusWeeks(1).with(java.time.DayOfWeek.MONDAY)
        fun entry(first: LocalDate) = io.github.naighes.scout4all.core.SeriesDraft(
            first.toString(), monday.plusWeeks(2).toString(),
            listOf(io.github.naighes.scout4all.core.SeriesBand(first.dayOfWeek.value.toUByte(), "20:30", 120.toUShort(), null)),
        )
        return core { it.createSeries(team, listOf(entry(monday), entry(monday.plusDays(3))), false) }.created
    }

    /** A Thursday of a series changed for the whole series: the Thursdays
     * only; a title asks nothing. */
    @Test
    fun aSessionOfASeriesAsksWhereItsDurationGoes() {
        val made = twoEntries()
        val thursday = made[1]
        val saved = mutableListOf<String>()
        rule.setContent { Scout4AllTheme { SessionFormScreen(team, thursday.id, onBack = {}, onSaved = { saved += it }) } }
        rule.waitForText(string(R.string.tr_session_duration))
        rule.type(string(R.string.tr_session_duration), "60")
        rule.save()
        rule.waitForText(string(R.string.tr_in_series_title))
        rule.onNode(hasText(string(R.string.tr_scope_all))).performClick()
        inDialog(string(R.string.save))
        rule.waitFor({ "not saved" }) { saved.isNotEmpty() }
        val minutes = core { it.trainingSessions(team) }.associate { it.start to it.durationMin.toInt() }
        assertEquals(
            made.associate { it.start to if (it.seriesId == thursday.seriesId) 60 else 120 },
            minutes,
        )
    }

    /** Deleting a session of a series asks which ones go. */
    @Test
    fun deletingASessionOfASeriesAsksWhichOnesGo() {
        val made = twoEntries()
        val monday = made[0]
        rule.setContent {
            Scout4AllTheme {
                SessionScreen(team, monday.id, onBack = {}, onEdit = {}, onActivity = {}, onAttendance = {}, onDuplicate = {})
            }
        }
        rule.waitForText(string(R.string.tr_attendance))
        rule.onNode(hasContentDescription(string(R.string.delete)) and hasClickAction()).performClick()
        rule.waitForText(string(R.string.tr_in_series_title))
        rule.onNode(hasText(string(R.string.tr_scope_all))).performClick()
        inDialog(string(R.string.delete))
        rule.waitFor({ "not deleted" }) { core { it.trainingSessions(team) }.size < made.size }
        val left = core { it.trainingSessions(team) }
        assertTrue(left.isNotEmpty())
        assertTrue(left.none { it.seriesId == monday.seriesId })
    }

    /** The sessions are in the calendar, no tab of their own: its list has
     * only the days with something, the filter hides what it leaves out. */
    @Test
    fun theListOfTheCalendarHasTheBusyDaysAndTheFilterHides() {
        val first = session(LocalDate.now().plusDays(1), "Ricezione")
        val second = session(LocalDate.now().plusDays(3), "Battuta")
        core { it.setSessionStatus(second, SessionStatus.CANCELLED) }
        hub(Part.CALENDAR)
        rule.waitForText(string(R.string.agenda_today))
        val tab = androidx.compose.ui.test.SemanticsMatcher.expectValue(
            androidx.compose.ui.semantics.SemanticsProperties.Role,
            androidx.compose.ui.semantics.Role.Tab,
        )
        assertEquals(0, rule.onAllNodes(hasText(string(R.string.tr_sessions)) and tab).fetchSemanticsNodes().size)
        press(string(R.string.tr_calendar_list))
        rule.waitForText("Ricezione")
        rule.waitForText("Battuta")
        // the day in between, empty, is not there
        val empty = LocalDate.now().plusDays(2)
        assertEquals(0, rule.onAllNodes(hasText(fullDayText(empty))).fetchSemanticsNodes().size)
        rule.onNode(hasText(fullDayText(LocalDate.now().plusDays(1)))).assertExists()
        // the chip, not the row of the cancelled session
        val chip = androidx.compose.ui.test.SemanticsMatcher.expectValue(
            androidx.compose.ui.semantics.SemanticsProperties.Role,
            androidx.compose.ui.semantics.Role.Checkbox,
        )
        rule.onNode(hasText(string(R.string.tr_filter_cancelled)) and chip).performScrollTo().performClick()
        rule.waitForIdle()
        rule.waitFor({ "still planned" }) { rule.onAllNodes(hasText("Ricezione")).fetchSemanticsNodes().isEmpty() }
        rule.onNode(hasText("Battuta")).assertExists()
        assertEquals(first, core { it.trainingSessions(team) }.first { s -> s.title == "Ricezione" }.id)
    }

    /** What is to close is in the past: the list looks at any date for it,
     * whatever period it was on. */
    @Test
    fun toCloseLooksAtAnyDateInTheList() {
        session(LocalDate.now().minusMonths(3), "Vecchia")
        hub(Part.CALENDAR)
        rule.waitForText(string(R.string.agenda_today))
        press(string(R.string.tr_calendar_list))
        rule.waitForText("Vecchia")
        press(string(R.string.tr_period_any))
        rule.onNode(hasText(string(R.string.tr_this_month)) and hasClickAction()).performClick()
        rule.waitFor({ "still there" }) { rule.onAllNodes(hasText("Vecchia")).fetchSemanticsNodes().isEmpty() }
        rule.onNode(hasText(string(R.string.tr_filter_to_close)) and hasClickAction()).performScrollTo().performClick()
        rule.waitForText("Vecchia")
    }

    /** "No attendance" lets through the completed sessions whose attendance
     * is not written, marked so in their row; the list looks at any date
     * for them, like "to close". */
    @Test
    fun noAttendanceShowsTheCompletedSessionsWithoutIt() {
        val bare = session(LocalDate.now().minusMonths(3), "Scoperta")
        core { it.completeSession(bare, emptyList()) }
        val taken = session(LocalDate.now().minusMonths(3).plusDays(1), "Rilevata")
        val rows = core { it.sessionAttendance(taken) }.rows
        core { it.saveAttendance(taken, rows.map { r -> r.copy(status = AttendanceStatus.PRESENT) }) }
        core { it.completeSession(taken, emptyList()) }
        session(LocalDate.now().plusDays(1), "Futura")
        hub(Part.CALENDAR)
        rule.waitForText(string(R.string.agenda_today))
        press(string(R.string.tr_calendar_list))
        rule.waitForText("Futura")
        press(string(R.string.tr_period_any))
        rule.onNode(hasText(string(R.string.tr_this_month)) and hasClickAction()).performClick()
        rule.waitFor({ "still there" }) { rule.onAllNodes(hasText("Scoperta")).fetchSemanticsNodes().isEmpty() }
        rule.onNode(hasText(string(R.string.tr_filter_no_attendance)) and hasClickAction()).performScrollTo().performClick()
        rule.waitForText("Scoperta")
        rule.waitForText(string(R.string.tr_attendance_not_recorded), substring = true)
        assertEquals(0, rule.onAllNodes(hasText("Rilevata")).fetchSemanticsNodes().size)
        assertEquals(0, rule.onAllNodes(hasText("Futura")).fetchSemanticsNodes().size)
    }

    @Test
    fun theAttendanceReportCountsTheCompletedSessions() {
        val s = session(LocalDate.now().minusDays(1))
        val rows = core { it.sessionAttendance(s) }.rows
        core { it.saveAttendance(s, rows.mapIndexed { i, r -> r.copy(status = if (i == 0) AttendanceStatus.ABSENT else AttendanceStatus.PRESENT) }) }
        core { it.completeSession(s, emptyList()) }
        hub(Part.ATTENDANCE)
        // the first is absent once, present never: 0% on one session
        rule.waitForText("0% ${string(R.string.tr_on_sessions)} 1")
        rule.waitForText(rows.first().name)
    }

    @Test
    fun theHistorySaysWhenThereAreNoEarlierSeasons() {
        hub(Part.HISTORY)
        rule.waitForText(string(R.string.tr_no_earlier_seasons).replaceFirstChar { it.uppercase() })
    }

    @Test
    fun theHistoryShowsTheEarlierSeasonsOfTheSameName() {
        // the team is Italia 2026: another name or a later year isn't there
        val old = saveTeam("Italia", 2025)
        saveTeam("Italia", 2027)
        saveTeam("Francia", 2024)
        val s = core { it.saveSession(old, null, it.newSessionDraft().copy(start = "2025-03-02T18:00", duration = "90", title = "Battuta")) }.id
        core { it.completeSession(s, emptyList()) }
        core { it.saveCycle(old, Level.PROGRAM, null, it.newCycleDraft().copy(title = "Andata", start = "2025-03-01", end = "2025-03-31"), emptyList(), false) }
        hub(Part.HISTORY)
        rule.waitForText("${string(R.string.season)} 2025")
        rule.waitForText("Andata")
        rule.waitForText("${string(R.string.tr_actual_time)}: 1:30")
        assertEquals(0, rule.onAllNodes(hasText("2027", substring = true)).fetchSemanticsNodes().size)
        assertEquals(0, rule.onAllNodes(hasText("2024", substring = true)).fetchSemanticsNodes().size)
    }

    private fun mark(name: String) {
        rule.onNode(isToggleable() and hasContentDescription(string(R.string.tr_mark, name))).performScrollTo().performClick()
        rule.waitForIdle()
    }

    /** The buttons over the list say their text only in the unmerged tree. */
    private fun fabs(text: String): Int =
        rule.onAllNodes(hasText(text), useUnmergedTree = true).fetchSemanticsNodes().size

    private fun waitForFab(text: String) = rule.waitFor({ "no button \"$text\"" }) { fabs(text) > 0 }

    private fun pressFab(text: String) {
        rule.onNode(hasText(text), useUnmergedTree = true).performClick()
        rule.waitForIdle()
    }

    private fun marks(name: String): Int =
        rule.onAllNodes(isToggleable() and hasContentDescription(string(R.string.tr_mark, name))).fetchSemanticsNodes().size

    private fun microcycle(title: String, monday: LocalDate, sessions: List<String> = emptyList()) = core {
        it.saveCycle(team, Level.MICROCYCLE, null, it.newCycleDraft().copy(title = title, start = monday.toString(), weeks = "1"), sessions, false)
    }.id

    /** The free sessions of the team have a box in the list and in the
     * month; what is marked stays across views, and "Create microcycle (N)"
     * shows only then and hands them over. A session in a microcycle has
     * no box. */
    @Test
    fun markedSessionsMakeAMicrocycle() {
        val monday = LocalDate.now().let { it.minusDays((it.dayOfWeek.value - 1).toLong()) }.plusWeeks(1)
        val first = session(monday, "Ricezione")
        val second = session(monday.plusDays(2), "Battuta")
        val held = session(monday.plusDays(3), "Attacco")
        microcycle("Carico", monday, listOf(held))
        val made = mutableListOf<Pair<Level, List<String>>>()
        hub(Part.CALENDAR, onCreateFrom = { level, ids -> made += level to ids })
        rule.waitForText(string(R.string.agenda_today))
        press(string(R.string.tr_calendar_list))
        rule.waitForText("Attacco")
        assertEquals(0, marks("Attacco"))
        val create = { n: Int -> "${string(R.string.tr_create_microcycle)} ($n)" }
        assertEquals(0, fabs(create(1)))
        mark("Ricezione")
        waitForFab(create(1))
        // the month keeps the mark: the second marked on its day
        press(string(R.string.tr_calendar_month))
        val day = monday.plusDays(2)
        if (java.time.YearMonth.from(day) != java.time.YearMonth.now()) {
            val next = hasContentDescription(string(R.string.agenda_next_week))
            rule.onNode(androidx.compose.ui.test.hasScrollToNodeAction()).performScrollToNode(next)
            rule.onNode(next).performClick()
            rule.waitForIdle()
        }
        waitForFab(create(1))
        val spoken = day.format(java.time.format.DateTimeFormatter.ofLocalizedDate(java.time.format.FormatStyle.FULL))
        val cell = androidx.compose.ui.test.hasTestTag(DAY_OF_MONTH) and hasText(spoken, substring = true)
        rule.onNode(androidx.compose.ui.test.hasScrollToNodeAction()).performScrollToNode(cell)
        rule.onNode(cell).performClick()
        rule.waitForText("Battuta")
        mark("Battuta")
        pressFab(create(2))
        rule.waitFor({ "not handed over" }) { made.isNotEmpty() }
        assertEquals(Level.MICROCYCLE, made.single().first)
        assertEquals(setOf(first, second), made.single().second.toSet())
        // handed over, the marks go
        rule.waitFor({ "still marked" }) { fabs(create(2)) == 0 }
    }

    /** The week by the hour: a session in its day's column, as tall as it
     * lasts (90 minutes, an hour and a half of the hours on the left); a
     * touch on it opens it, one on an empty half hour makes a session on
     * that day at that time. */
    @Test
    fun theWeekByTheHourOpensASessionAndMakesOneAtAHalfHour() {
        val today = LocalDate.now()
        val held = session(today, "Ricezione")
        val at = mutableListOf<Pair<LocalDate, LocalTime>>()
        val opened = mutableListOf<String>()
        hub(Part.CALENDAR, onNewAt = { d, t -> at += d to t }, onSession = { opened += it })
        rule.waitForText(string(R.string.agenda_today))
        press(string(R.string.tr_calendar_week))
        val card = hasTestTag(WEEK_EVENT) and hasText("Ricezione", substring = true)
        rule.onNode(card).performScrollTo()
        val hour = rule.onNode(hasTestTag("$WEEK_HOUR/18")).fetchSemanticsNode().size.height
        val tall = rule.onNode(card).fetchSemanticsNode().size.height
        assertTrue("$tall is not an hour and a half of $hour", kotlin.math.abs(tall - hour * 3 / 2) <= hour / 10)
        rule.onNode(card).performClick()
        rule.waitFor({ "not opened" }) { opened.isNotEmpty() }
        assertEquals(listOf(held), opened)
        // not read by TalkBack (the + does the same): touched, not clicked
        val slot = rule.onNode(hasTestTag("$WEEK_SLOT/${today}T20:30"))
        slot.performScrollTo()
        slot.performTouchInput { click() }
        rule.waitFor({ "no new session" }) { at.isNotEmpty() }
        assertEquals(listOf(today to LocalTime.of(20, 30)), at)
    }

    /** Two sessions at the same time (two teams, all of them shown) go side
     * by side in their day, each half of it. */
    @Test
    fun sessionsAtTheSameTimeGoSideBySideInTheWeek() {
        val today = LocalDate.now()
        session(today, "Ricezione")
        val other = saveTeam("Arezzo")
        core { it.saveSession(other, null, it.newSessionDraft().copy(start = "${today}T18:00", duration = "90", title = "Battuta")) }
        hub(Part.CALENDAR)
        rule.waitForText(string(R.string.agenda_today))
        press(string(R.string.venue_this_team))
        press(string(R.string.tr_calendar_week))
        val card = { title: String -> hasTestTag(WEEK_EVENT) and hasText(title, substring = true) }
        rule.onNode(card("Battuta")).performScrollTo()
        val a = rule.onNode(card("Ricezione")).fetchSemanticsNode().boundsInRoot
        val b = rule.onNode(card("Battuta")).fetchSemanticsNode().boundsInRoot
        val (left, right) = if (a.left < b.left) a to b else b to a
        assertTrue("$left and $right overlap", left.right <= right.left)
        assertEquals(a.top, b.top, 1f)
    }

    /** The free microcycles have a box; "Create mesocycle (N)" makes one of
     * the marked; the programs, with no level above, have none. */
    @Test
    fun markedCyclesMakeTheLevelAbove() {
        val monday = LocalDate.now().let { it.minusDays((it.dayOfWeek.value - 1).toLong()) }
        val a = microcycle("Carico", monday)
        val b = microcycle("Scarico", monday.plusWeeks(1))
        val c = microcycle("Tenuto", monday.plusWeeks(2))
        core {
            it.saveCycle(team, Level.MESOCYCLE, null, it.newCycleDraft().copy(title = "Blocco", start = monday.plusWeeks(2).toString(), weeks = "1"), listOf(c), false)
        }
        val made = mutableListOf<Pair<Level, List<String>>>()
        hub(Part.CYCLES, onCreateFrom = { level, ids -> made += level to ids })
        rule.waitForText("Carico")
        assertEquals(0, marks("Tenuto"))
        mark("Carico")
        mark("Scarico")
        pressFab("${string(R.string.tr_create_mesocycle)} (2)")
        rule.waitFor({ "not handed over" }) { made.isNotEmpty() }
        assertEquals(Level.MESOCYCLE, made.single().first)
        assertEquals(setOf(a, b), made.single().second.toSet())
        rule.onNode(hasText(string(Level.PROGRAM.plural())) and hasClickAction()).performClick()
        rule.waitForIdle()
        assertEquals(0, rule.onAllNodes(isToggleable() and hasContentDescription(string(R.string.tr_mark, "Blocco"), substring = true)).fetchSemanticsNodes().size)
    }

    /** A new microcycle of sessions starts on the Monday of the first and
     * lasts to the week of the last; saved, it holds them. */
    @Test
    fun aMicrocycleOfSessionsIsProposedAndHoldsThem() {
        val monday = LocalDate.now().let { it.minusDays((it.dayOfWeek.value - 1).toLong()) }.plusWeeks(1)
        val first = session(monday.plusDays(1), "Ricezione")
        val second = session(monday.plusWeeks(1).plusDays(3), "Battuta")
        val saved = mutableListOf<String>()
        rule.setContent {
            Scout4AllTheme {
                CycleFormScreen(team, Level.MICROCYCLE, null, onBack = {}, onSaved = { saved += it }, children = listOf(second, first))
            }
        }
        rule.waitForText(string(R.string.tr_cycle_weeks))
        rule.onNode(hasSetTextAction() and hasText("2")).assertExists()
        rule.save()
        rule.waitFor({ "not saved" }) { saved.isNotEmpty() }
        val cycle = core { it.cycle(Level.MICROCYCLE, saved.single()) }
        assertEquals(monday.toString(), cycle.firstDay)
        assertEquals(setOf(first, second), core { it.cycleChildren(Level.MICROCYCLE, cycle.id) }.map { c -> c.id }.toSet())
    }

    /** A new activity copies one of another session: the name searched,
     * a touch on it fills the form, saved it has the description and the
     * minutes of the one copied. */
    @Test
    fun anActivityIsCopiedFromAnotherSession() {
        val other = session(LocalDate.now().minusDays(7), "Prima")
        core {
            it.saveActivity(other, null, it.newActivityDraft().copy(name = "Palleggio", description = "a coppie", planned = "25"))
        }
        val s = session()
        var done = false
        rule.setContent { Scout4AllTheme { ActivityFormScreen(team, s, null, onBack = {}, onDone = { done = true }) } }
        rule.waitForText(string(R.string.tr_copy_activity))
        rule.type(string(R.string.tr_copy_activity), "pall")
        rule.waitForText("Palleggio")
        rule.onNode(hasText("Palleggio") and hasClickAction()).performClick()
        rule.waitFor({ "not filled" }) { rule.onAllNodes(hasText("a coppie")).fetchSemanticsNodes().isNotEmpty() }
        rule.save()
        rule.waitFor({ "not saved" }) { done }
        val copied = core { it.sessionActivities(s) }.single()
        assertEquals("Palleggio", copied.name)
        assertEquals("a coppie", copied.description)
        assertEquals(25.toUShort(), copied.plannedMin)
    }

    /** The share of the plan says first that the notes on the players go
     * with it; shared, the archive holds the sessions and the cycles. */
    @Test
    fun theTrainingIsSharedAsAnArchive() {
        val monday = LocalDate.now().plusWeeks(2).let { it.minusDays((it.dayOfWeek.value - 1).toLong()) }
        val held = session(monday, "Ricezione")
        core {
            it.saveCycle(team, Level.MICROCYCLE, null, it.newCycleDraft().copy(start = monday.toString(), weeks = "1"), listOf(held), false)
        }
        session(monday.plusDays(10), "Libera")
        hub(Part.CALENDAR)
        rule.waitForText(string(R.string.agenda_today))
        rule.onNode(hasContentDescription(string(R.string.tr_export_training)) and hasClickAction()).performClick()
        rule.waitForText(string(R.string.tr_training_notes_warning))
        rule.onNode(
            hasContentDescription(string(R.string.tr_export_training)) and hasClickAction() and hasAnyAncestor(isDialog()),
        ).performClick()
        rule.onNode(hasText(string(R.string.share)) and hasClickAction()).performClick()
        val app = ApplicationProvider.getApplicationContext<Scout4AllApplication>()
        var chooser: Intent? = null
        rule.waitFor({ "not shared" }) {
            chooser = shadowOf(app).nextStartedActivity
            chooser != null
        }
        @Suppress("DEPRECATION")
        val uri = chooser?.getParcelableExtra<Intent>(Intent.EXTRA_INTENT)?.getParcelableExtra<Uri>(Intent.EXTRA_STREAM)
        val bytes = app.contentResolver.openInputStream(uri!!)!!.use { it.readBytes() }
        val preview = core { it.previewArchive(bytes, null, null) }
        assertTrue("$preview", preview is ArchivePreview.Training)
        val training = preview as ArchivePreview.Training
        assertEquals(2u, training.sessions)
        assertEquals(1u, training.cycles)
    }

    /** The sheet of a mesocycle counts the sessions of its microcycles by
     * state, the time planned (not of the cancelled) and the time done
     * (the planned one when not written). */
    @Test
    fun theSheetOfAMesocycleTotalsTheSessionsOfItsMicrocycles() {
        val monday = LocalDate.now().plusWeeks(2).let { it.minusDays((it.dayOfWeek.value - 1).toLong()) }
        val done = session(monday, "Fatta")
        val cancelled = session(monday.plusDays(1), "Annullata")
        val planned = session(monday.plusDays(8), "Prevista")
        session(monday.plusDays(3), "Libera")
        core { it.completeSession(done, emptyList()) }
        core { it.setSessionStatus(cancelled, SessionStatus.CANCELLED) }
        val micros = listOf(listOf(done, cancelled), listOf(planned)).mapIndexed { i, held ->
            core {
                it.saveCycle(
                    team, Level.MICROCYCLE, null,
                    it.newCycleDraft().copy(start = monday.plusWeeks(i.toLong()).toString(), weeks = "1"), held, false,
                ).id
            }
        }
        val meso = core {
            it.saveCycle(team, Level.MESOCYCLE, null, it.newCycleDraft().copy(start = monday.toString(), weeks = "2"), micros, false)
        }
        rule.setContent {
            Scout4AllTheme {
                CycleScreen(
                    team, Level.MESOCYCLE, meso.id,
                    onBack = {}, onEdit = {}, onAdd = {}, onDuplicate = {}, onChild = {}, onNewChild = {}, onMatch = {},
                )
            }
        }
        rule.waitForText(
            listOf(
                "${string(R.string.tr_filter_planned)} 1",
                "${string(R.string.tr_filter_completed)} 1",
                "${string(R.string.tr_filter_cancelled)} 1",
            ).joinToString(" · "),
        )
        rule.waitForText("${string(R.string.tr_planned_time)}: 3:00")
        rule.waitForText("${string(R.string.tr_actual_time)}: 1:30")
    }
}
