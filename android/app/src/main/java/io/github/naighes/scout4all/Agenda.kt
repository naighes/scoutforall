package io.github.naighes.scout4all

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.AgendaDay
import io.github.naighes.scout4all.core.AgendaEventRecord
import io.github.naighes.scout4all.core.AgendaItemRecord
import io.github.naighes.scout4all.core.AgendaSpan
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

/** A session or a match: the time, what, where; the match opens. */
@Composable
internal fun EventRow(
    e: AgendaEventRecord,
    withTeam: Boolean,
    onMatch: (teamId: String, matchId: String) -> Unit,
    onSession: (teamId: String, sessionId: String) -> Unit,
    lit: Boolean = false,
    unrecorded: Boolean = false,
    mark: Pair<Boolean, () -> Unit>? = null,
) {
    // on today's background the rows are clear, to let it show
    val colors = if (lit) ListItemDefaults.colors(containerColor = Color.Transparent) else ListItemDefaults.colors()
    val time = e.start.substringAfter('T', "").take(5)
    when (val i = e.item) {
        is AgendaItemRecord.Session -> {
            val cancelled = i.status == SessionStatus.CANCELLED
            ListItem(
                colors = colors,
                modifier = Modifier.clickable { onSession(e.teamId, i.id) },
                overlineContent = { Text(listOfNotNull(time, e.teamName.takeIf { withTeam && i.title.isNotBlank() }).joinToString(" · ")) },
                headlineContent = {
                    Text(
                        i.title.ifBlank { e.teamName },
                        textDecoration = if (cancelled) TextDecoration.LineThrough else null,
                    )
                },
                supportingContent = {
                    val note = when {
                        cancelled -> stringResource(R.string.agenda_cancelled)
                        i.toClose -> stringResource(R.string.agenda_to_close)
                        else -> null
                    }
                    val missing = stringResource(R.string.tr_attendance_not_recorded).takeIf { unrecorded }
                    val line = listOfNotNull(e.venueText(), note, missing).joinToString(" · ")
                    if (line.isNotEmpty()) Text(line, color = e.clashColor())
                },
                // the box on the right: the rows without it (matches,
                // sessions already in a microcycle) stay aligned
                trailingContent = if (e.sharedVenue || mark != null) {
                    {
                        Row(verticalAlignment = Alignment.CenterVertically) {
                            if (e.sharedVenue) ClashMark()
                            mark?.let { (marked, toggle) -> MarkBox(i.title.ifBlank { e.teamName }, marked, toggle) }
                        }
                    }
                } else null,
            )
        }
        is AgendaItemRecord.Match -> ListItem(
            colors = colors,
            modifier = Modifier.clickable { onMatch(e.teamId, i.id) },
            overlineContent = { Text(listOfNotNull(time, e.teamName.takeIf { withTeam }).joinToString(" · ")) },
            headlineContent = { Text(i.opponent) },
            supportingContent = e.venueText()?.let { { Text(it, color = e.clashColor()) } },
            trailingContent = {
                if (e.sharedVenue) ClashMark()
                val ours = i.ours
                val theirs = i.theirs
                if (i.played && ours != null && theirs != null) {
                    Text(
                        "$ours–$theirs",
                        Modifier.readAs(stringResource(R.string.sets_score, ours.toInt(), theirs.toInt())),
                        style = MaterialTheme.typography.titleMedium,
                        fontWeight = FontWeight.SemiBold,
                        color = resultColor(true, ours.toInt(), theirs.toInt()),
                    )
                }
            },
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
