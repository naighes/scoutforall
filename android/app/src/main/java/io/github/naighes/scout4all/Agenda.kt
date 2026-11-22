package io.github.naighes.scout4all

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
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

/** The events of today, only when there is something: a title and the rows. */
fun LazyListScope.agendaToday(
    days: List<AgendaDay>,
    withTeam: Boolean,
    onMatch: (teamId: String, matchId: String) -> Unit,
    onSession: (teamId: String, sessionId: String) -> Unit = { _, _ -> },
) {
    val events = days.flatMap { it.events }
    if (events.isEmpty()) return
    item(key = "agenda-today") { AgendaTitle(R.string.agenda_today) }
    items(events, key = { "today/${it.teamId}/${it.start}/${it.id()}" }) { EventRow(it, withTeam, onMatch, onSession) }
}

/** The week of [monday]: every day, the empty ones too, with the buttons
 * to move it. */
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
    // the phone is narrow: only the days with something, or a line saying
    // there is nothing
    val busy = days.filter { it.events.isNotEmpty() }
    if (busy.isEmpty()) {
        item(key = "agenda-nothing") {
            Text(
                stringResource(R.string.agenda_nothing),
                Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
    busy.forEach { day ->
        val date = LocalDate.parse(day.date)
        item(key = "day/${day.date}") { DayHeader(date, date == today) }
        items(day.events, key = { "${day.date}/${it.teamId}/${it.start}/${it.id()}" }) { EventRow(it, withTeam, onMatch, onSession) }
    }
}

internal fun AgendaEventRecord.id(): String = when (val i = item) {
    is AgendaItemRecord.Session -> i.id
    is AgendaItemRecord.Match -> i.id
}

@Composable
private fun AgendaTitle(title: Int) = Text(
    stringResource(title),
    Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 4.dp).title(),
    style = MaterialTheme.typography.titleSmall,
    color = MaterialTheme.colorScheme.primary,
)

/** The day, today in the accent. */
@Composable
internal fun DayHeader(date: LocalDate, today: Boolean) = Text(
    date.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.FULL).withLocale(java.util.Locale.getDefault())),
    Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp),
    style = MaterialTheme.typography.labelLarge,
    fontWeight = if (today) FontWeight.Bold else FontWeight.Normal,
    color = if (today) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
)

/** A session or a match: the time, what, where; the match opens. */
@Composable
internal fun EventRow(
    e: AgendaEventRecord,
    withTeam: Boolean,
    onMatch: (teamId: String, matchId: String) -> Unit,
    onSession: (teamId: String, sessionId: String) -> Unit,
) {
    val time = e.start.substringAfter('T', "").take(5)
    when (val i = e.item) {
        is AgendaItemRecord.Session -> {
            val cancelled = i.status == SessionStatus.CANCELLED
            ListItem(
                modifier = Modifier.clickable { onSession(e.teamId, i.id) },
                overlineContent = { Text(listOfNotNull(time, e.teamName.takeIf { withTeam }).joinToString(" · ")) },
                headlineContent = {
                    Text(
                        i.title.ifBlank { stringResource(R.string.agenda_training) },
                        textDecoration = if (cancelled) TextDecoration.LineThrough else null,
                    )
                },
                supportingContent = {
                    val note = when {
                        cancelled -> stringResource(R.string.agenda_cancelled)
                        i.toClose -> stringResource(R.string.agenda_to_close)
                        else -> null
                    }
                    val line = listOfNotNull(i.location.takeIf { it.isNotBlank() }, note).joinToString(" · ")
                    if (line.isNotEmpty()) Text(line)
                },
            )
        }
        is AgendaItemRecord.Match -> ListItem(
            modifier = Modifier.clickable { onMatch(e.teamId, i.id) },
            overlineContent = { Text(listOfNotNull(time, e.teamName.takeIf { withTeam }).joinToString(" · ")) },
            headlineContent = { Text(i.opponent) },
            trailingContent = {
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
