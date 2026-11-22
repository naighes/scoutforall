package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.isToggleable
import androidx.compose.ui.test.performClick
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
                team, onBack = {}, onSession = {}, onNew = {}, onSeries = {}, onCycle = { _, _ -> },
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

    @Test
    fun theCalendarOpensANewSessionOnTheDay() {
        val days = mutableListOf<LocalDate>()
        hub(Part.CALENDAR, onNewOn = { days += it })
        rule.waitForText(string(R.string.agenda_today))
        rule.onNode(hasContentDescription(string(R.string.tr_new_session)) and hasClickAction()).performClick()
        assertEquals(listOf(LocalDate.now()), days)
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
