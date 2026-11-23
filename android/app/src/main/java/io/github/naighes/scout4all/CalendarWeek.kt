package io.github.naighes.scout4all

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.sizeIn
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.naighes.scout4all.core.AgendaDay
import io.github.naighes.scout4all.core.AgendaEventRecord
import io.github.naighes.scout4all.core.AgendaItemRecord
import io.github.naighes.scout4all.core.SessionStatus
import io.github.naighes.scout4all.core.weekLayout
import java.time.LocalDate
import java.time.LocalTime
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle
import java.util.Locale

/** The tag of an event of the week by the hour. */
internal const val WEEK_EVENT = "week-event"

/** The tag of the head of a day of the week (its number): seven in a row,
 * as the days of the month. */
internal const val WEEK_DAY = "week-day"

/** The tag of a half hour of the week, followed by `/YYYY-MM-DDTHH:MM`: a
 * touch makes a session there. Not read by TalkBack (the + does the same). */
internal const val WEEK_SLOT = "week-slot"

/** The tag of an hour on the left, followed by `/H`. */
internal const val WEEK_HOUR = "week-hour"

/** The height of an hour: a half hour is a touch away from the next. */
private val HOUR = 48.dp

/** The width of the hours on the left: two digits of their text, larger
 * with the font. */
@Composable
private fun gutter(): Dp = with(LocalDensity.current) { 22.sp.toDp() } + 6.dp

/** The least height of an event, so it can be read and touched. */
private val MIN_EVENT = 20.dp

/** The minute the week by the hour opens at: an hour before its first
 * event, the first hour shown if it has none. Pixels from the top of
 * `WeekHours`. */
@Composable
internal fun weekOpening(days: List<AgendaDay>): Int {
    val layout = weekLayout(days)
    val first = layout.firstHour.toInt()
    val earliest = layout.days.flatten().minOfOrNull { it.start.toInt() } ?: return 0
    val hours = (earliest / 60 - first - 1).coerceAtLeast(0)
    return with(LocalDensity.current) { (HOUR * hours).roundToPx() }
}

/** The heads of the week by the hour: the short name and the number of
 * each day over its column, today in the accent, the chosen one in a
 * circle; a touch chooses it. On the background, to stay on top of the
 * hours scrolling under it. */
@Composable
internal fun WeekHeads(dates: List<LocalDate>, selected: LocalDate, today: LocalDate, onDay: (LocalDate) -> Unit) {
    Row(Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.surface).padding(end = 4.dp, top = 4.dp)) {
        Spacer(Modifier.width(gutter()))
        dates.forEach { date -> DayHead(date, date == selected, date == today, Modifier.weight(1f)) { onDay(date) } }
    }
}

/** The week as the calendars of the mail draw it, the same logic as the
 * desktop (`weekLayout`): the days side by side, the hours on the left,
 * each event as tall as it lasts, the ones at the same time side by side.
 * A touch on an event opens it (`onEvent`), a long one marks a free
 * session (`mark`); one on an empty half hour makes a session there. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun WeekHours(
    days: List<AgendaDay>,
    today: LocalDate,
    onEvent: (AgendaEventRecord) -> Unit,
    onNewAt: (LocalDate, LocalTime) -> Unit,
    mark: (AgendaEventRecord) -> Pair<Boolean, () -> Unit>?,
) {
    val layout = weekLayout(days)
    val first = layout.firstHour.toInt()
    val last = layout.lastHour.toInt()
    val line = MaterialTheme.colorScheme.outlineVariant
    Row(Modifier.fillMaxWidth().padding(end = 4.dp)) {
        Column(Modifier.width(gutter())) {
            (first until last).forEach { hour ->
                Box(Modifier.height(HOUR).fillMaxWidth().testTag("$WEEK_HOUR/$hour"), contentAlignment = Alignment.TopEnd) {
                    Text(
                        "$hour",
                        Modifier.padding(end = 4.dp),
                        style = MaterialTheme.typography.labelSmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
        days.zip(layout.days).forEach { (day, places) ->
            val date = LocalDate.parse(day.date)
            BoxWithConstraints(
                Modifier
                    .weight(1f)
                    .height(HOUR * (last - first))
                    .background(if (date == today) MaterialTheme.colorScheme.primary.copy(alpha = 0.06f) else Color.Transparent)
                    .drawBehind { drawLine(line, Offset(0f, 0f), Offset(0f, size.height), 1f) },
            ) {
                Column {
                    (first until last).forEach { hour ->
                        listOf(0, 30).forEach { minute ->
                            Box(
                                Modifier
                                    .fillMaxWidth()
                                    .height(HOUR / 2)
                                    .drawBehind {
                                        if (minute == 0) drawLine(line, Offset(0f, 0f), Offset(size.width, 0f), 1f)
                                    }
                                    .testTag("$WEEK_SLOT/${date}T%02d:%02d".format(hour, minute))
                                    .pointerInput(date, hour, minute) {
                                        detectTapGestures { onNewAt(date, LocalTime.of(hour, minute)) }
                                    },
                            )
                        }
                    }
                }
                val column = maxWidth
                day.events.zip(places).forEach { (event, place) ->
                    val width = column / place.lanes.toInt()
                    val top = HOUR * ((place.start.toInt() - first * 60).coerceAtLeast(0) / 60f)
                    val tall = maxOf(HOUR * (place.minutes.toInt() / 60f), MIN_EVENT)
                    EventCard(
                        event,
                        mark(event),
                        Modifier
                            .offset(x = width * place.lane.toInt(), y = top)
                            .size(width, tall)
                            .padding(1.dp),
                        onClick = { onEvent(event) },
                    )
                }
            }
        }
    }
}

/** A day of the week on top: its short name and its number, today in the
 * accent, the chosen one in a circle. */
@Composable
private fun DayHead(date: LocalDate, selected: Boolean, today: Boolean, modifier: Modifier, onClick: () -> Unit) {
    Column(
        modifier
            .testTag(WEEK_DAY)
            .clickable(onClick = onClick)
            .readAs(fullDayText(date))
            .padding(vertical = 2.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(
            date.dayOfWeek.getDisplayName(TextStyle.SHORT, Locale.getDefault()),
            style = MaterialTheme.typography.labelSmall,
            color = if (today) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Clip,
        )
        Box(
            Modifier
                .sizeIn(minWidth = 30.dp, minHeight = 30.dp)
                .background(if (selected) MaterialTheme.colorScheme.primaryContainer else Color.Transparent, CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Text(
                "${date.dayOfMonth}",
                maxLines = 1,
                softWrap = false,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = if (today) FontWeight.Bold else FontWeight.Normal,
                color = if (today) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
            )
        }
    }
}

/** An event in its column: a bar of its color on the left (sessions and
 * matches apart, as the dots of the month), its time and what it is, cut
 * by the box; read whole. A gym held by another team at the same time in
 * the error color, a marked session with its check. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun EventCard(event: AgendaEventRecord, mark: Pair<Boolean, () -> Unit>?, modifier: Modifier, onClick: () -> Unit) {
    val session = event.item as? AgendaItemRecord.Session
    val color = when {
        event.sharedVenue -> MaterialTheme.colorScheme.error
        session != null -> MaterialTheme.colorScheme.primary
        else -> MaterialTheme.colorScheme.tertiary
    }
    val cancelled = session?.status == SessionStatus.CANCELLED
    val time = event.start.substringAfter('T', "").take(5)
    val what = when (val i = event.item) {
        is AgendaItemRecord.Session -> i.title.ifBlank { event.teamName }
        is AgendaItemRecord.Match -> i.opponent
    }
    val notes = listOfNotNull(
        stringResource(R.string.agenda_cancelled).takeIf { cancelled },
        stringResource(R.string.agenda_to_close).takeIf { session?.toClose == true },
        event.venueText(),
        stringResource(R.string.venue_busy_mark).takeIf { event.sharedVenue },
    )
    val spoken = (listOf(time, what) + notes).joinToString(", ")
    val shape = MaterialTheme.shapes.extraSmall
    Box(
        modifier
            .clip(shape)
            .background(color.copy(alpha = if (cancelled) 0.08f else 0.18f))
            .then(if (mark?.first == true) Modifier.border(2.dp, color, shape) else Modifier)
            .drawBehind { drawRect(color, size = size.copy(width = 3.dp.toPx())) }
            .testTag(WEEK_EVENT)
            .combinedClickable(onClick = onClick, onLongClick = mark?.second)
            .readAs(spoken)
            .padding(start = 5.dp, end = 1.dp, top = 1.dp),
    ) {
        Column {
            Text(time, fontSize = 10.sp, lineHeight = 11.sp, maxLines = 1, overflow = TextOverflow.Clip, softWrap = false)
            Text(
                what,
                fontSize = 11.sp,
                lineHeight = 12.sp,
                fontWeight = FontWeight.Medium,
                textDecoration = if (cancelled) TextDecoration.LineThrough else null,
                overflow = TextOverflow.Clip,
            )
        }
        if (mark?.first == true) {
            Icon(Icons.Filled.Check, null, Modifier.align(Alignment.TopEnd).size(12.dp), tint = color)
        }
    }
}

/** The week of a day as its header reads it, the days being under it:
 * its month ("November 2026"), both when it runs into the next ("Nov –
 * Dec 2026"). */
internal fun weekText(day: LocalDate): String {
    val monday = day.minusDays((day.dayOfWeek.value - 1).toLong())
    val sunday = monday.plusDays(6)
    val locale = Locale.getDefault()
    val written = { d: LocalDate, pattern: String -> d.format(DateTimeFormatter.ofPattern(pattern, locale)) }
    val text = when {
        monday.month == sunday.month -> written(monday, "LLLL yyyy")
        monday.year == sunday.year -> "${written(monday, "LLL")} – ${written(sunday, "LLL yyyy")}"
        else -> "${written(monday, "LLL yyyy")} – ${written(sunday, "LLL yyyy")}"
    }
    return text.replaceFirstChar { it.uppercase() }
}
