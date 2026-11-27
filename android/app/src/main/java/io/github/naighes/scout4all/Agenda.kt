package io.github.naighes.scout4all

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.takeOrElse
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.AgendaDay
import io.github.naighes.scout4all.core.AgendaEventRecord
import io.github.naighes.scout4all.core.AgendaItemRecord
import io.github.naighes.scout4all.core.AgendaSpan
import io.github.naighes.scout4all.core.AppointmentsRecord
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.SessionStatus
import java.time.LocalDate
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/** The days of the agenda around [day]: of the team, or of every team
 * when [teamId] is null. Empty until read, and when it can't be. */
@Composable
fun rememberAgenda(teamId: String?, span: AgendaSpan, day: LocalDate, refresh: Any?): State<List<AgendaDay>> {
    val app = androidx.compose.ui.platform.LocalContext.current.applicationContext as Scout4AllApplication
    return produceState(emptyList(), teamId, span, day, refresh) {
        value = try {
            app.core { it.trainingAgenda(teamId, span, day.toString()) }
        } catch (_: MobileException) {
            emptyList()
        }
    }
}

/** The days with something between [from] and [to] (both included, null
 * for no end), of the team or of every team: the list of the calendar.
 * Read only while [reading]; empty until read, and when it can't be. */
@Composable
fun rememberAgendaBetween(teamId: String?, from: LocalDate?, to: LocalDate?, reading: Boolean): State<List<AgendaDay>> {
    val app = androidx.compose.ui.platform.LocalContext.current.applicationContext as Scout4AllApplication
    return produceState(emptyList(), teamId, from, to, reading) {
        value = if (!reading) emptyList() else try {
            app.core { it.trainingAgendaBetween(teamId, from?.toString(), to?.toString()) }
        } catch (_: MobileException) {
            emptyList()
        }
    }
}

/** The week of [monday], with the buttons to move it: the days with
 * something planned, and today, always there and lit. */
fun LazyListScope.agendaWeek(
    days: List<AgendaDay>,
    today: LocalDate,
    withTeam: Boolean,
    onPrevious: () -> Unit,
    onNext: () -> Unit,
    onToday: () -> Unit,
    onMatch: (teamId: String, matchId: String) -> Unit,
    onSession: (teamId: String, sessionId: String) -> Unit = { _, _ -> },
) {
    item(key = "agenda-week") {
        Row(
            Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                stringResource(R.string.agenda_week),
                Modifier.weight(1f).title(),
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.primary,
            )
            IconButton(onClick = onPrevious) {
                Icon(Icons.AutoMirrored.Filled.KeyboardArrowLeft, contentDescription = stringResource(R.string.agenda_previous_week))
            }
            TextButton(onClick = onToday) { Text(stringResource(R.string.agenda_today)) }
            IconButton(onClick = onNext) {
                Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = stringResource(R.string.agenda_next_week))
            }
        }
    }
    // the phone is narrow: only the days with something and today, or a
    // line saying there is nothing
    val shown = days.filter { it.events.isNotEmpty() || it.date == today.toString() }
    if (shown.isEmpty()) {
        item(key = "agenda-nothing") { NothingPlanned() }
    }
    shown.forEach { day ->
        val date = LocalDate.parse(day.date)
        if (date == today) {
            // today as one block, on a background of its own
            item(key = "day/${day.date}") {
                TodayBox {
                    DayHeader(date, true)
                    if (day.events.isEmpty()) NothingPlanned()
                    day.events.forEach { EventRow(it, withTeam, onMatch, onSession, lit = true) }
                }
            }
        } else {
            item(key = "day/${day.date}") { DayHeader(date, false) }
            items(day.events, key = { "${day.date}/${it.teamId}/${it.start}/${it.id()}" }) { EventRow(it, withTeam, onMatch, onSession) }
        }
    }
}

/** Today's appointments of the team and its next official matches; null
 * until read, and when they can't be. */
@Composable
fun rememberAppointments(teamId: String, refresh: Any?): State<AppointmentsRecord?> {
    val app = androidx.compose.ui.platform.LocalContext.current.applicationContext as Scout4AllApplication
    return produceState<AppointmentsRecord?>(null, teamId, refresh) {
        value = try {
            app.core { it.teamAppointments(teamId) }
        } catch (_: MobileException) {
            null
        }
    }
}

/** What the team has today, in a small box of its own: an official match
 * stands out. Several events scroll in it, the list below keeps its room. */
@Composable
internal fun TodayAppointments(
    events: List<AgendaEventRecord>,
    onMatch: (teamId: String, matchId: String) -> Unit,
    onSession: (teamId: String, sessionId: String) -> Unit,
) = TodayBox {
    Text(
        stringResource(R.string.agenda_today),
        Modifier.padding(start = 16.dp, top = 6.dp).title(),
        style = MaterialTheme.typography.labelLarge,
        fontWeight = FontWeight.Bold,
        color = MaterialTheme.colorScheme.primary,
    )
    Column(Modifier.heightIn(max = TODAY_MAX).verticalScroll(rememberScrollState())) {
        events.forEach { EventRow(it, false, onMatch, onSession, lit = true, featured = it.official()) }
    }
}

/** The next official match, with the way to the calendar. */
@Composable
internal fun NextMatch(
    match: AgendaEventRecord,
    onCalendar: () -> Unit,
    onMatch: (teamId: String, matchId: String) -> Unit,
) = Column(Modifier.testTag(NEXT_MATCH_TAG)) {
    Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(
            stringResource(R.string.agenda_next_match),
            // the button right after it: at the end the + would cover it
            Modifier.weight(1f, fill = false).title(),
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.primary,
        )
        TextButton(onClick = onCalendar) { Text(stringResource(R.string.agenda_see_calendar)) }
    }
    DayHeader(LocalDate.parse(match.start.substringBefore('T')), false)
    EventRow(match, false, onMatch, { _, _ -> }, featured = true)
}

/** The test tag of the box of the next match. */
const val NEXT_MATCH_TAG = "agenda-next-match"

private val TODAY_MAX = 168.dp

private fun AgendaEventRecord.official() = (item as? AgendaItemRecord.Match)?.friendly == false

/** The line of a day, or a week, without anything. */
@Composable
internal fun NothingPlanned() = Text(
    stringResource(R.string.agenda_nothing),
    Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
    style = MaterialTheme.typography.bodyMedium,
    color = MaterialTheme.colorScheme.onSurfaceVariant,
)

/** Today, lit: its rows on a light background of the accent. */
@Composable
internal fun TodayBox(content: @Composable ColumnScope.() -> Unit) = Column(
    Modifier
        .fillMaxWidth()
        .padding(horizontal = 8.dp, vertical = 4.dp)
        .background(MaterialTheme.colorScheme.primary.copy(alpha = 0.08f), RoundedCornerShape(12.dp))
        .testTag(TODAY_TAG),
    content = content,
)

/** The test tag of today's block. */
const val TODAY_TAG = "agenda-today"


internal fun AgendaEventRecord.id(): String = when (val i = item) {
    is AgendaItemRecord.Session -> i.id
    is AgendaItemRecord.Match -> i.id
}

/** The day, today in the accent. */
@Composable
internal fun DayHeader(date: LocalDate, today: Boolean) = Text(
    fullDayText(date),
    Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp),
    style = MaterialTheme.typography.labelLarge,
    fontWeight = if (today) FontWeight.Bold else FontWeight.Normal,
    color = if (today) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
)

/** A day as its header reads it: the whole date, in the language. */
internal fun fullDayText(date: LocalDate): String =
    date.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.FULL).withLocale(java.util.Locale.getDefault()))

/** A session or a match: the time at its head over the icon, what, where,
 * the score of a match at its tail; a touch opens it. */
@Composable
internal fun EventRow(
    e: AgendaEventRecord,
    withTeam: Boolean,
    onMatch: (teamId: String, matchId: String) -> Unit,
    onSession: (teamId: String, sessionId: String) -> Unit,
    lit: Boolean = false,
    unrecorded: Boolean = false,
    mark: Pair<Boolean, () -> Unit>? = null,
    // an official match told apart: in words and with a frame
    featured: Boolean = false,
) {
    val item = e.item
    val session = item as? AgendaItemRecord.Session
    val cancelled = session?.status == SessionStatus.CANCELLED
    val title = when (item) {
        is AgendaItemRecord.Session -> item.title.ifBlank { e.teamName }
        is AgendaItemRecord.Match -> item.opponent
    }
    // a session without a title is already called by its team
    val team = e.teamName.takeIf { withTeam && session?.title?.isBlank() != true }
    val note = when {
        cancelled -> stringResource(R.string.agenda_cancelled)
        session?.toClose == true -> stringResource(R.string.agenda_to_close)
        else -> null
    }
    val missing = stringResource(R.string.tr_attendance_not_recorded).takeIf { unrecorded }
    val detail = listOfNotNull(e.venueText(), note, missing).joinToString(" · ")
    BoxRow(
        onClick = {
            when (item) {
                is AgendaItemRecord.Session -> onSession(e.teamId, item.id)
                is AgendaItemRecord.Match -> onMatch(e.teamId, item.id)
            }
        },
        // today's block has a margin of its own: the rows stay aligned
        modifier = Modifier.padding(horizontal = if (lit) 4.dp else 12.dp, vertical = 4.dp),
        border = BorderStroke(2.dp, BreakPoint).takeIf { featured },
        head = { EventHead(e.start.substringAfter('T', "").take(5), session != null) },
        tail = { EventTail(e, title, mark) },
    ) {
        team?.let { Text(it, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        if (featured) {
            Text(stringResource(R.string.agenda_official_match), style = MaterialTheme.typography.labelMedium, color = BreakPoint)
        }
        Text(
            title,
            style = MaterialTheme.typography.bodyLarge,
            textDecoration = if (cancelled) TextDecoration.LineThrough else null,
        )
        if (detail.isNotEmpty()) {
            Text(
                detail,
                style = MaterialTheme.typography.bodyMedium,
                color = e.clashColor().takeOrElse { MaterialTheme.colorScheme.onSurfaceVariant },
            )
        }
    }
}

/** The time of an event over the icon that tells a session from a match. */
@Composable
private fun EventHead(time: String, session: Boolean) = Column(horizontalAlignment = Alignment.CenterHorizontally) {
    Text(time, style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
    Icon(
        painterResource(if (session) R.drawable.ic_calendar else R.drawable.ic_ball),
        contentDescription = null,
        Modifier.size(20.dp),
        tint = if (session) MaterialTheme.colorScheme.primary else BreakPoint,
    )
}

/** The sign of a shared gym, the box of a free session, the score. */
@Composable
private fun EventTail(e: AgendaEventRecord, title: String, mark: Pair<Boolean, () -> Unit>?) =
    Row(Modifier.padding(end = if (mark == null) 12.dp else 4.dp), verticalAlignment = Alignment.CenterVertically) {
        if (e.sharedVenue) ClashMark()
        mark?.let { (marked, toggle) -> MarkBox(title, marked, toggle) }
        val match = e.item as? AgendaItemRecord.Match
        val ours = match?.ours
        val theirs = match?.theirs
        if (match?.played == true && ours != null && theirs != null) {
            Text(
                "$ours–$theirs",
                Modifier.padding(start = 8.dp).readAs(stringResource(R.string.sets_score, ours.toInt(), theirs.toInt())),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.SemiBold,
                color = resultColor(true, ours.toInt(), theirs.toInt()),
            )
        }
    }

/** The name of the gym of an event, "deleted gym" for a removed one. */
@Composable
internal fun AgendaEventRecord.venueText(): String? =
    venue?.let { if (it.deleted) stringResource(R.string.venue_deleted) else it.name }

/** The error colour of an event whose gym another team holds too. */
@Composable
internal fun AgendaEventRecord.clashColor() =
    if (sharedVenue) MaterialTheme.colorScheme.error else androidx.compose.ui.graphics.Color.Unspecified

/** The sign of a gym held by two teams at once. */
@Composable
internal fun ClashMark() = Icon(
    Icons.Filled.Warning,
    contentDescription = stringResource(R.string.venue_busy_mark),
    tint = MaterialTheme.colorScheme.error,
)
