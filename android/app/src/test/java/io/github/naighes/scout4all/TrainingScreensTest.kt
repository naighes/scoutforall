package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.naighes.scout4all.core.ActivityStatus
import io.github.naighes.scout4all.core.AttendanceStatus
import io.github.naighes.scout4all.core.Level
import io.github.naighes.scout4all.core.SessionStatus
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.time.LocalDate

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
                    team, session, onBack = {}, onEdit = {}, onActivity = {}, onAttendance = {}, onDuplicate = {}, onSeries = {},
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

    private fun hub(part: Part, onNewOn: (LocalDate) -> Unit = {}, onNewCycle: (Level) -> Unit = {}) = rule.setContent {
        Scout4AllTheme {
            TrainingScreen(
                team, onBack = {}, onSession = {}, onSeries = {}, onCycle = { _, _ -> },
                onNewCycle = onNewCycle, onNewOn = onNewOn, onMatch = {}, first = part,
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
        rule.onNode(hasContentDescription(string(R.string.tr_new_session)) and hasClickAction()).performClick()
        assertEquals(listOf(LocalDate.now()), days)
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
}
