package io.github.naighes.scout4all

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ScrollableTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.AgendaSpan
import io.github.naighes.scout4all.core.AgendaItemRecord
import io.github.naighes.scout4all.core.CycleRecord
import io.github.naighes.scout4all.core.Level
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.TrainingScope
import io.github.naighes.scout4all.core.AgendaDay
import io.github.naighes.scout4all.core.AgendaEventRecord
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.YearMonth
import java.time.format.DateTimeFormatter
import java.time.format.TextStyle
import java.util.Locale

internal enum class Part { CALENDAR, CYCLES, ATTENDANCE, HISTORY }

/** The calendar as the month (a day to touch, what it holds below) or as
 * the list of the days with something, for a long period. */
internal enum class CalendarView { WEEK, MONTH, LIST }

/** What the calendar lets through: everything, the sessions only, the
 * sessions of a state (the matches have none), or the completed ones of
 * the team whose attendance is not written. */
private enum class EventFilter {
    ALL, SESSIONS, PLANNED, TO_CLOSE, COMPLETED, CANCELLED, NO_ATTENDANCE;

    /** What is left to close or to take the attendance of is in the past:
     * the list looks at any date for it. */
    val looksBack get() = this == TO_CLOSE || this == NO_ATTENDANCE
}

/** The days of the list; null for any date. */
private enum class ListPeriod { ANY, WEEK, MONTH, PREVIOUS_MONTH, NEXT_MONTH }

private fun ListPeriod.days(today: LocalDate): Pair<LocalDate, LocalDate>? = when (this) {
    ListPeriod.ANY -> null
    ListPeriod.WEEK -> weekOf(today)
    ListPeriod.MONTH -> YearMonth.from(today).let { it.atDay(1) to it.atEndOfMonth() }
    ListPeriod.PREVIOUS_MONTH -> YearMonth.from(today).minusMonths(1).let { it.atDay(1) to it.atEndOfMonth() }
    ListPeriod.NEXT_MONTH -> YearMonth.from(today).plusMonths(1).let { it.atDay(1) to it.atEndOfMonth() }
}

@Composable
private fun ListPeriod.label(): String = stringResource(
    when (this) {
        ListPeriod.ANY -> R.string.tr_period_any
        ListPeriod.WEEK -> R.string.tr_this_week
        ListPeriod.MONTH -> R.string.tr_this_month
        ListPeriod.PREVIOUS_MONTH -> R.string.tr_previous_month
        ListPeriod.NEXT_MONTH -> R.string.tr_next_month
    },
)

@Composable
private fun EventFilter.label(): String = stringResource(
    when (this) {
        EventFilter.ALL -> R.string.tr_events_all
        EventFilter.SESSIONS -> R.string.tr_sessions
        EventFilter.PLANNED -> R.string.tr_filter_planned
        EventFilter.TO_CLOSE -> R.string.tr_filter_to_close
        EventFilter.COMPLETED -> R.string.tr_filter_completed
        EventFilter.CANCELLED -> R.string.tr_filter_cancelled
        EventFilter.NO_ATTENDANCE -> R.string.tr_filter_no_attendance
    },
)

/** [unrecorded]: whether [e] is a completed session of the team without
 * its attendance (another team's is not read). */
private fun EventFilter.lets(e: AgendaEventRecord, unrecorded: (AgendaEventRecord) -> Boolean): Boolean {
    val s = e.item as? AgendaItemRecord.Session
    return when (this) {
        EventFilter.ALL -> true
        EventFilter.SESSIONS -> s != null
        EventFilter.PLANNED -> s?.status == io.github.naighes.scout4all.core.SessionStatus.PLANNED
        EventFilter.TO_CLOSE -> s?.toClose == true
        EventFilter.COMPLETED -> s?.status == io.github.naighes.scout4all.core.SessionStatus.COMPLETED
        EventFilter.CANCELLED -> s?.status == io.github.naighes.scout4all.core.SessionStatus.CANCELLED
        EventFilter.NO_ATTENDANCE -> unrecorded(e)
    }
}

/** The completed sessions of [teamId] among [days] whose attendance is
 * written: the others are the ones without it. Null until read, so
 * that nothing shows as missing before it is known. */
@Composable
private fun rememberRecorded(teamId: String, days: List<List<AgendaDay>>): androidx.compose.runtime.State<Set<String>?> {
    val app = androidx.compose.ui.platform.LocalContext.current.applicationContext as Scout4AllApplication
    val ids = days.flatten().flatMap { it.events }
        .filter { it.teamId == teamId }
        .mapNotNull { (it.item as? AgendaItemRecord.Session)?.takeIf { s -> s.status == io.github.naighes.scout4all.core.SessionStatus.COMPLETED }?.id }
        .toSet()
    return produceState<Set<String>?>(null, teamId, ids) {
        value = try {
            app.core { c -> ids.filter { c.sessionAttendance(it).recorded }.toSet() }
        } catch (_: io.github.naighes.scout4all.core.MobileException) {
            emptySet()
        }
    }
}

/** The training of a team in three parts: the calendar (which lists the
 * sessions too), the cycles and the attendance. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun TrainingScreen(
    teamId: String,
    onBack: () -> Unit,
    onSession: (String) -> Unit,
    onCycle: (Level, String) -> Unit,
    onNewCycle: (Level) -> Unit,
    onNewOn: (LocalDate) -> Unit,
    onMatch: (String) -> Unit,
    onVenues: () -> Unit = {},
    onNewAt: (LocalDate, java.time.LocalTime) -> Unit = { d, _ -> onNewOn(d) },
    onNewMatch: (LocalDate) -> Unit = {},
    first: Part = Part.CALENDAR,
    firstView: CalendarView = CalendarView.MONTH,
    onCreateFrom: (Level, List<String>) -> Unit = { _, _ -> },
) {
    var part by rememberSaveable { mutableStateOf(first) }
    var view by rememberSaveable { mutableStateOf(firstView) }
    var level by rememberSaveable { mutableStateOf(Level.MICROCYCLE) }
    var day by rememberSaveable { mutableStateOf(LocalDate.now()) }
    var sharing by rememberSaveable { mutableStateOf(false) }
    // what is marked to make a cycle of it: the sessions stay across the
    // views and periods of the calendar, the cycles while on their level
    var sessions by rememberSaveable { mutableStateOf(emptyList<String>()) }
    var cycles by rememberSaveable { mutableStateOf(emptyList<String>()) }
    val free by rememberCore(part, teamId) { c -> c.trainingSessions(teamId).filter { it.microcycleId == null }.map { it.id }.toSet() }
    val markedSessions = free?.let { f -> sessions.filter { it in f } }.orEmpty()
    Scaffold(
        topBar = {
            Column {
                TopAppBar(
                    title = { Text(stringResource(R.string.tr_programming), Modifier.title()) },
                    navigationIcon = { BackButton(onBack) },
                    actions = {
                        // the gyms are where sessions are held: they're
                        // added from here too, not only from the home
                        IconButton(onClick = onVenues) {
                            Icon(
                                androidx.compose.ui.res.painterResource(R.drawable.ic_gym),
                                contentDescription = stringResource(R.string.venues),
                            )
                        }
                        IconButton(onClick = { sharing = true }) {
                            Icon(
                                Icons.Filled.Share,
                                contentDescription = stringResource(R.string.tr_export_training),
                            )
                        }
                    },
                )
                ScrollableTabRow(selectedTabIndex = part.ordinal, edgePadding = 8.dp) {
                    Part.entries.forEach {
                        Tab(
                            selected = part == it,
                            onClick = { part = it },
                            text = { Text(it.label()) },
                        )
                    }
                }
            }
        },
        floatingActionButton = {
            // said in words: the same + makes a session here, a cycle there;
            // above it, only once something is marked, the cycle of it
            val up = level.parent()
            when (part) {
                Part.CALENDAR -> Fabs(
                    markedSessions.takeIf { it.isNotEmpty() }?.let { m ->
                        "${stringResource(R.string.tr_create_microcycle)} (${m.size})" to {
                            onCreateFrom(Level.MICROCYCLE, m)
                            sessions = emptyList()
                        }
                    },
                    stringResource(R.string.tr_new_session),
                ) { onNewOn(day) }
                Part.CYCLES -> Fabs(
                    up?.takeIf { cycles.isNotEmpty() }?.let { u ->
                        "${stringResource(u.createName())} (${cycles.size})" to {
                            onCreateFrom(u, cycles)
                            cycles = emptyList()
                        }
                    },
                    stringResource(level.createName()),
                ) { onNewCycle(level) }
                else -> {}
            }
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (part) {
            Part.CALENDAR -> CalendarTab(
                teamId, day, { day = it }, view, { view = it }, modifier, onSession, onMatch, onNewOn,
                free.orEmpty(), markedSessions, { sessions = it }, onNewAt, onNewMatch,
            )
            Part.CYCLES -> CyclesTab(
                teamId, level,
                {
                    level = it
                    cycles = emptyList()
                },
                modifier, onCycle, cycles, { cycles = it },
            )
            Part.ATTENDANCE -> AttendanceTab(teamId, modifier)
            Part.HISTORY -> HistoryTab(teamId, modifier)
        }
    }
    if (sharing) {
        // the notes on the players go with it: said before
        AlertDialog(
            onDismissRequest = { sharing = false },
            title = { Text(stringResource(R.string.tr_export_training)) },
            text = { Text(stringResource(R.string.tr_training_notes_warning)) },
            confirmButton = { ExportButton(stringResource(R.string.tr_export_training)) { it.exportTraining(teamId) } },
            dismissButton = { TextButton(onClick = { sharing = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

/** The + of the part, and above it the cycle of what is marked, when
 * something is: both said in words. */
@Composable
private fun Fabs(create: Pair<String, () -> Unit>?, add: String, onAdd: () -> Unit) =
    Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(16.dp)) {
        create?.let { (label, onCreate) ->
            ExtendedFloatingActionButton(
                onClick = onCreate,
                containerColor = MaterialTheme.colorScheme.secondaryContainer,
                icon = { Icon(Icons.Filled.Add, contentDescription = null) },
                text = { Text(label) },
            )
        }
        ExtendedFloatingActionButton(
            onClick = onAdd,
            icon = { Icon(Icons.Filled.Add, contentDescription = add) },
            text = { Text(add) },
        )
    }

/** Room under a list for the buttons over it: one, or two with something
 * marked. */
internal fun fabRoom(marked: Boolean) = if (marked) 160.dp else 88.dp

@Composable
private fun Part.label(): String = when (this) {
    Part.CALENDAR -> stringResource(R.string.tr_calendar).replaceFirstChar { it.uppercase() }
    Part.CYCLES -> stringResource(R.string.tr_programming)
    Part.ATTENDANCE -> stringResource(R.string.tr_attendance)
    Part.HISTORY -> stringResource(R.string.tr_history)
}

// ---- calendar

/** Where the heads of the week (with the period) are in the calendar's
 * list: after the views and the filters, before the hours. */
private const val WEEK_HEADS_ITEM = 2

/** The week by the hour (`WeekHours`), the month (a day to touch, a dot
 * for each session and match; below, what the selected day holds) or the
 * list of the days with something. The filter hides what it leaves out in
 * all three. */
@OptIn(androidx.compose.foundation.ExperimentalFoundationApi::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
private fun CalendarTab(
    teamId: String,
    selected: LocalDate,
    onSelected: (LocalDate) -> Unit,
    view: CalendarView,
    onView: (CalendarView) -> Unit,
    modifier: Modifier,
    onSession: (String) -> Unit,
    onMatch: (String) -> Unit,
    onNewOn: (LocalDate) -> Unit,
    free: Set<String> = emptySet(),
    marked: List<String> = emptyList(),
    onMarked: (List<String>) -> Unit = {},
    onNewAt: (LocalDate, java.time.LocalTime) -> Unit = { d, _ -> onNewOn(d) },
    onNewMatch: (LocalDate) -> Unit = {},
) {
    var month by rememberSaveable { mutableStateOf(YearMonth.from(selected)) }
    // the gyms are shared: every team, or one gym only
    var allTeams by rememberSaveable { mutableStateOf(false) }
    var venue by rememberSaveable { mutableStateOf<String?>(null) }
    var filter by rememberSaveable { mutableStateOf(EventFilter.ALL) }
    var period by rememberSaveable { mutableStateOf(ListPeriod.ANY) }
    val today = LocalDate.now()
    val team = teamId.takeIf { !allTeams }
    val monthDays by rememberAgenda(team, AgendaSpan.MONTH, month.atDay(1), month)
    val weekDays by rememberAgenda(team, AgendaSpan.WEEK, selected, weekOf(selected).first)
    val bounds = period.days(today)
    val listDays by rememberAgendaBetween(team, bounds?.first, bounds?.second, view == CalendarView.LIST)
    val recorded by rememberRecorded(teamId, listOf(monthDays, weekDays, listDays))
    val unrecorded = { e: AgendaEventRecord ->
        val s = e.item as? AgendaItemRecord.Session
        e.teamId == teamId && s?.status == io.github.naighes.scout4all.core.SessionStatus.COMPLETED &&
            recorded?.let { s.id !in it } == true
    }
    fun shown(days: List<AgendaDay>) =
        days.map { d -> d.copy(events = d.events.filter { (venue == null || it.venue?.id == venue) && filter.lets(it, unrecorded) }) }
    val byDate = shown(monthDays).associateBy { it.date }
    val app = androidx.compose.ui.platform.LocalContext.current.applicationContext as Scout4AllApplication
    val venues by produceState(emptyList<io.github.naighes.scout4all.core.VenueRecord>()) {
        value = try {
            app.core { it.venues() }
        } catch (_: io.github.naighes.scout4all.core.MobileException) {
            emptyList()
        }
    }
    var picking by remember { mutableStateOf(false) }
    var periods by remember { mutableStateOf(false) }
    // the free sessions of the team, to make a microcycle of them
    val mark = { e: AgendaEventRecord ->
        (e.item as? AgendaItemRecord.Session)?.id?.takeIf { e.teamId == teamId && it in free }?.let { id ->
            (id in marked) to { onMarked(if (id in marked) marked - id else marked + id) }
        }
    }
    // the other teams' events are shown, not opened
    val openEvent = { e: AgendaEventRecord ->
        when (val i = e.item) {
            is AgendaItemRecord.Session -> if (e.teamId == teamId) onSession(i.id)
            is AgendaItemRecord.Match -> if (e.teamId == teamId) onMatch(i.id)
        }
    }
    val open: @Composable (AgendaEventRecord) -> Unit = { e ->
        EventRow(
            e,
            allTeams,
            { t, m -> if (t == teamId) onMatch(m) },
            { t, s -> if (t == teamId) onSession(s) },
            unrecorded = unrecorded(e),
            mark = mark(e),
        )
    }
    // the period and its arrows: the month, or the week of the day chosen
    val heading: @Composable () -> Unit = {
        Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
            Text(
                if (view == CalendarView.WEEK) weekText(selected)
                else month.format(DateTimeFormatter.ofPattern("LLLL yyyy", Locale.getDefault())).replaceFirstChar { it.uppercase() },
                Modifier.weight(1f).title(),
                style = MaterialTheme.typography.titleMedium,
                color = MaterialTheme.colorScheme.primary,
            )
            IconButton(onClick = {
                if (view == CalendarView.WEEK) onSelected(selected.minusWeeks(1).also { month = YearMonth.from(it) })
                else month = month.minusMonths(1)
            }) {
                Icon(Icons.AutoMirrored.Filled.KeyboardArrowLeft, contentDescription = stringResource(R.string.agenda_previous_week))
            }
            TextButton(onClick = {
                month = YearMonth.now()
                onSelected(today)
            }) { Text(stringResource(R.string.agenda_today)) }
            IconButton(onClick = {
                if (view == CalendarView.WEEK) onSelected(selected.plusWeeks(1).also { month = YearMonth.from(it) })
                else month = month.plusMonths(1)
            }) {
                Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = stringResource(R.string.agenda_next_week))
            }
        }
    }
    val views: @Composable () -> Unit = {
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp, vertical = 4.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            FilterChip(
                selected = view == CalendarView.WEEK,
                onClick = { onView(CalendarView.WEEK) },
                label = { Text(stringResource(R.string.tr_calendar_week)) },
            )
            FilterChip(
                selected = view == CalendarView.MONTH,
                onClick = {
                    month = YearMonth.from(selected)
                    onView(CalendarView.MONTH)
                },
                label = { Text(stringResource(R.string.tr_calendar_month)) },
            )
            FilterChip(
                selected = view == CalendarView.LIST,
                onClick = { onView(CalendarView.LIST) },
                label = { Text(stringResource(R.string.tr_calendar_list)) },
            )
            if (view == CalendarView.LIST) {
                Box {
                    FilterChip(
                        selected = period != ListPeriod.ANY,
                        onClick = { periods = true },
                        label = { Text(period.label()) },
                    )
                    androidx.compose.material3.DropdownMenu(expanded = periods, onDismissRequest = { periods = false }) {
                        ListPeriod.entries.forEach { p ->
                            androidx.compose.material3.DropdownMenuItem(
                                text = { Text(p.label()) },
                                onClick = { period = p; periods = false },
                            )
                        }
                    }
                }
            }
            FilterChip(
                selected = allTeams,
                onClick = { allTeams = !allTeams },
                label = { Text(stringResource(if (allTeams) R.string.venue_all_teams else R.string.venue_this_team)) },
            )
            if (venues.isNotEmpty()) {
                Box {
                    FilterChip(
                        selected = venue != null,
                        onClick = { picking = true },
                        label = { Text(venues.firstOrNull { it.id == venue }?.name ?: stringResource(R.string.venue_all)) },
                    )
                    androidx.compose.material3.DropdownMenu(expanded = picking, onDismissRequest = { picking = false }) {
                        androidx.compose.material3.DropdownMenuItem(
                            text = { Text(stringResource(R.string.venue_all)) },
                            onClick = { venue = null; picking = false },
                        )
                        venues.forEach { v ->
                            androidx.compose.material3.DropdownMenuItem(
                                text = { Text(v.name) },
                                onClick = { venue = v.id; picking = false },
                            )
                        }
                    }
                }
            }
        }
    }
    val filters: @Composable () -> Unit = {
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp),
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            EventFilter.entries.forEach {
                FilterChip(
                    selected = filter == it,
                    onClick = {
                        filter = it
                        if (it.looksBack && view == CalendarView.LIST) period = ListPeriod.ANY
                    },
                    label = { Text(it.label()) },
                )
            }
        }
    }
    val week = shown(weekDays)
    val list = androidx.compose.foundation.lazy.rememberLazyListState()
    // a week opens at its first event: the heads stay on top of the hours
    val opening = weekOpening(week)
    androidx.compose.runtime.LaunchedEffect(view, weekOf(selected).first, opening) {
        // from the heads: what is past them goes under them, right below
        if (view == CalendarView.WEEK && opening > 0) list.scrollToItem(WEEK_HEADS_ITEM, opening)
    }
    LazyColumn(modifier, list, contentPadding = PaddingValues(bottom = fabRoom(marked.isNotEmpty()))) {
        if (view == CalendarView.MONTH) item(key = "month") { heading() }
        item(key = "views") { views() }
        item(key = "filters") { filters() }
        when (view) {
            CalendarView.WEEK -> {
                // the week and its arrows stay on top with the days
                stickyHeader(key = "week-heads") {
                    Column(Modifier.background(MaterialTheme.colorScheme.surface)) {
                        heading()
                        WeekHeads(week.map { LocalDate.parse(it.date) }, selected, today, onSelected)
                    }
                }
                item(key = "week-hours") { WeekHours(week, today, openEvent, onNewAt, mark) }
            }
            CalendarView.MONTH -> {
                item(key = "grid") {
                    MonthGrid(month, selected, today, byDate, onSelected)
                }
                item(key = "day") { DayHeader(selected, selected == today) }
                val events = byDate[selected.toString()]?.events.orEmpty()
                if (events.isEmpty()) {
                    item(key = "nothing") {
                        Text(
                            stringResource(R.string.agenda_nothing),
                            Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                items(events, key = { "${it.teamId}/${it.start}/${it.id()}" }) { open(it) }
                // a match on the day chosen, right under it, named as on the
                // desktop; the session on it is the floating button's
                item(key = "add") {
                    TextButton(onClick = { onNewMatch(selected) }, Modifier.padding(horizontal = 8.dp)) {
                        Icon(Icons.Filled.Add, contentDescription = null)
                        Text(stringResource(R.string.new_match), Modifier.padding(start = 8.dp))
                    }
                }
            }
            CalendarView.LIST -> {
                // only the days with something, the filter too
                val days = shown(listDays).filter { it.events.isNotEmpty() }
                item(key = "count") {
                    Text(
                        countText(null, days.sumOf { d -> d.events.count { it.item is AgendaItemRecord.Session } }),
                        Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (days.isEmpty()) {
                    item(key = "nothing") {
                        Text(
                            stringResource(R.string.tr_no_sessions),
                            Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                days.forEach { d ->
                    val date = LocalDate.parse(d.date)
                    item(key = "list-${d.date}") { DayHeader(date, date == today) }
                    items(d.events, key = { "list/${it.teamId}/${it.start}/${it.id()}" }) { open(it) }
                }
            }
        }
    }
}

@Composable
private fun MonthGrid(month: YearMonth, selected: LocalDate, today: LocalDate, byDate: Map<String, AgendaDay>, onDay: (LocalDate) -> Unit) {
    val lead = month.atDay(1).dayOfWeek.value - 1 // Monday first
    val cells = lead + month.lengthOfMonth()
    val rows = (cells + 6) / 7
    Column(Modifier.padding(horizontal = 8.dp)) {
        Row(Modifier.fillMaxWidth()) {
            DayOfWeek.entries.forEach {
                Text(
                    it.getDisplayName(TextStyle.NARROW, Locale.getDefault()),
                    Modifier.weight(1f),
                    textAlign = TextAlign.Center,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        repeat(rows) { r ->
            Row(Modifier.fillMaxWidth()) {
                repeat(7) { c ->
                    val n = r * 7 + c - lead + 1
                    if (n < 1 || n > month.lengthOfMonth()) {
                        // a day of the months around: only its number, in
                        // grey, as on the desktop
                        Box(Modifier.weight(1f).height(52.dp), contentAlignment = Alignment.Center) {
                            Text(
                                "${month.atDay(1).plusDays((n - 1).toLong()).dayOfMonth}",
                                Modifier.testTag(DAY_AROUND).clearAndSetSemantics {},
                                style = MaterialTheme.typography.bodyLarge,
                                color = MaterialTheme.colorScheme.outline,
                            )
                        }
                    } else {
                        val date = month.atDay(n)
                        DayCell(date, date == selected, date == today, byDate[date.toString()]?.events.orEmpty(), Modifier.weight(1f)) { onDay(date) }
                    }
                }
            }
        }
    }
}

/** The tag of a day of the month: seven in a row, a seventh of the width
 * each (45dp apart on a phone 320dp wide, as the days of Material's date
 * picker): the 48dp between neighbours can't hold there. */
internal const val DAY_OF_MONTH = "day-of-month"

/** The tag of a day of the months around, in grey in the month. */
internal const val DAY_AROUND = "day-around"

@Composable
private fun DayCell(date: LocalDate, selected: Boolean, today: Boolean, events: List<AgendaEventRecord>, modifier: Modifier, onClick: () -> Unit) {
    val items = events.map { it.item }
    val clash = events.any { it.sharedVenue }
    val sessions = items.count { it is AgendaItemRecord.Session }
    val matches = items.size - sessions
    val spoken = date.format(DateTimeFormatter.ofLocalizedDate(java.time.format.FormatStyle.FULL)) +
        if (items.isEmpty()) "" else ", ${items.size}"
    Column(
        modifier
            .height(52.dp)
            .padding(2.dp)
            .background(
                if (selected) MaterialTheme.colorScheme.primaryContainer else androidx.compose.ui.graphics.Color.Transparent,
                MaterialTheme.shapes.small,
            )
            // outside `readAs`, which clears what is inside it
            .testTag(DAY_OF_MONTH)
            .clickable(onClick = onClick)
            .readAs(spoken),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.Center,
    ) {
        Text(
            "${date.dayOfMonth}",
            style = MaterialTheme.typography.bodyLarge,
            fontWeight = if (today) FontWeight.Bold else FontWeight.Normal,
            color = if (today) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurface,
        )
        Row(Modifier.height(8.dp).padding(top = 2.dp), horizontalArrangement = Arrangement.spacedBy(3.dp)) {
            if (sessions > 0) Dot(MaterialTheme.colorScheme.primary)
            if (matches > 0) Dot(MaterialTheme.colorScheme.tertiary)
            if (clash) Dot(MaterialTheme.colorScheme.error)
        }
    }
}

@Composable
private fun Dot(color: androidx.compose.ui.graphics.Color) =
    Box(Modifier.size(6.dp).background(color, CircleShape))

// ---- attendance

private fun weekOf(day: LocalDate): Pair<LocalDate, LocalDate> {
    val monday = day.minusDays((day.dayOfWeek.value - 1).toLong())
    return monday to monday.plusDays(6)
}

private fun monthOf(month: YearMonth) = month.atDay(1).toString() to month.atEndOfMonth().toString()

/** The attendance of each player over a period or a cycle, with the
 * percentage; read only. */
@Composable
private fun AttendanceTab(teamId: String, modifier: Modifier) {
    var choice by rememberSaveable { mutableStateOf("all") }
    val cycles by rememberCore(Unit, teamId) { core ->
        listOf(Level.PROGRAM, Level.MESOCYCLE, Level.MICROCYCLE).flatMap { l -> core.cycles(teamId, l).map { l to it } }
    }
    val today = LocalDate.now()
    val scopes = buildList<Triple<String, String, TrainingScope>> {
        add(Triple("all", stringResource(R.string.tr_whole_season), TrainingScope.All))
        weekOf(today).let { (a, b) -> add(Triple("week", stringResource(R.string.tr_this_week), TrainingScope.Dates(a.toString(), b.toString()))) }
        monthOf(YearMonth.from(today)).let { (a, b) -> add(Triple("month", stringResource(R.string.tr_this_month), TrainingScope.Dates(a, b))) }
        monthOf(YearMonth.from(today).minusMonths(1)).let { (a, b) -> add(Triple("previous", stringResource(R.string.tr_previous_month), TrainingScope.Dates(a, b))) }
        cycles.orEmpty().forEach { (l, c) ->
            add(Triple("${l.name}/${c.id}", c.name(l), TrainingScope.Cycle(l, c.id)))
        }
    }
    val current = scopes.firstOrNull { it.first == choice } ?: scopes.first()
    val report by rememberCore(current.first, teamId) { it.attendanceReport(teamId, current.third) }
    Column(modifier) {
        ChoiceField(
            stringResource(R.string.tr_attendance),
            scopes.map { it.first to it.second },
            current.first,
            onSelect = { choice = it },
            modifier = Modifier.padding(16.dp),
        )
        val r = report
        // the sessions counted, none in words
        r?.let {
            Text(
                countText(null, it.sessions.toInt()),
                Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        when {
            r == null -> Centered(Modifier.fillMaxSize()) { CircularProgressIndicator() }
            r.players.isEmpty() -> Text(
                stringResource(R.string.tr_no_attendance_to_count).replaceFirstChar { it.uppercase() },
                Modifier.padding(horizontal = 16.dp),
            )
            else -> LazyColumn(Modifier.fillMaxSize()) {
                items(r.players, key = { it.personId }) { p ->
                    ListItem(
                        overlineContent = p.number?.let { { Text("$it") } },
                        headlineContent = { Text(p.name) },
                        supportingContent = {
                            Text(
                                listOf(
                                    "${stringResource(R.string.tr_present)} ${p.present}" +
                                        if (p.late > 0u) " (${stringResource(R.string.tr_late)} ${p.late})" else "",
                                    "${stringResource(R.string.tr_absent)} ${p.absent}",
                                    "${stringResource(R.string.tr_unavailable)} ${p.unavailable}",
                                    stringResource(R.string.makeups_count, p.makeups.toInt()).takeIf { p.makeups > 0u },
                                ).filterNotNull().joinToString(" · "),
                            )
                        },
                        trailingContent = {
                            Text(
                                p.percent?.let { "$it% ${stringResource(R.string.tr_on_sessions)} ${p.present + p.absent}" } ?: NO_CHOICE,
                                style = MaterialTheme.typography.titleSmall,
                            )
                        },
                    )
                }
            }
        }
    }
}

/** The training of the seasons before: the teams of the same name with an
 * earlier year, the latest first, each with its totals and its programs
 * with theirs. Read only: a season is planned on its own team. */
@Composable
private fun HistoryTab(teamId: String, modifier: Modifier) {
    val seasons by rememberCore(Unit, teamId) { it.trainingHistory(teamId) }
    val all = seasons
    when {
        all == null -> Centered(modifier) { CircularProgressIndicator() }
        all.isEmpty() -> Text(
            stringResource(R.string.tr_no_earlier_seasons).replaceFirstChar { it.uppercase() },
            modifier.padding(16.dp),
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        else -> LazyColumn(modifier) {
            all.forEach { season ->
                item(key = season.teamId) {
                    Column(
                        Modifier.fillMaxWidth().padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 8.dp),
                        verticalArrangement = Arrangement.spacedBy(4.dp),
                    ) {
                        Text(
                            "${stringResource(R.string.season)} ${season.year}",
                            Modifier.title(),
                            style = MaterialTheme.typography.titleMedium,
                        )
                        TotalsLines(season.totals)
                    }
                }
                if (season.programs.isEmpty()) {
                    item(key = "${season.teamId}/none") {
                        Text(
                            stringResource(R.string.tr_no_cycles).replaceFirstChar { it.uppercase() },
                            Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
                items(season.programs, key = { "${season.teamId}/${it.program.id}" }) { p ->
                    ListItem(
                        overlineContent = { Text(stringResource(R.string.tr_program)) },
                        headlineContent = { Text(p.program.title.ifBlank { p.program.window }) },
                        supportingContent = {
                            Column {
                                if (p.program.title.isNotBlank()) Text(p.program.window)
                                TotalsLines(p.totals)
                            }
                        },
                    )
                }
            }
        }
    }
}

/** A cycle by its title, or its window when it has none. */
@Composable
fun CycleRecord.name(level: Level): String =
    title.ifBlank { window } + " · " + stringResource(level.label())

fun Level.label(): Int = when (this) {
    Level.PROGRAM -> R.string.tr_program
    Level.MESOCYCLE -> R.string.tr_mesocycle
    Level.MICROCYCLE -> R.string.tr_microcycle
}

fun Level.plural(): Int = when (this) {
    Level.PROGRAM -> R.string.tr_programs
    Level.MESOCYCLE -> R.string.tr_mesocycles
    Level.MICROCYCLE -> R.string.tr_microcycles
}
