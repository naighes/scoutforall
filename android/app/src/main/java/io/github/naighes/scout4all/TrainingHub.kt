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
import androidx.compose.material3.FloatingActionButton
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

internal enum class Part { CALENDAR, SESSIONS, CYCLES, ATTENDANCE }

/** The training of a team in four parts: the calendar, the sessions, the
 * cycles and the attendance. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun TrainingScreen(
    teamId: String,
    onBack: () -> Unit,
    onSession: (String) -> Unit,
    onNew: () -> Unit,
    onSeries: () -> Unit,
    onCycle: (Level, String) -> Unit,
    onNewCycle: (Level) -> Unit,
    onNewOn: (LocalDate) -> Unit,
    onMatch: (String) -> Unit,
    first: Part = Part.SESSIONS,
) {
    var part by rememberSaveable { mutableStateOf(first) }
    var level by rememberSaveable { mutableStateOf(Level.MICROCYCLE) }
    var day by rememberSaveable { mutableStateOf(LocalDate.now()) }
    var sharing by rememberSaveable { mutableStateOf(false) }
    Scaffold(
        topBar = {
            Column {
                TopAppBar(
                    title = { Text(stringResource(R.string.tr_programming), Modifier.title()) },
                    navigationIcon = { BackButton(onBack) },
                    actions = {
                        if (part == Part.SESSIONS) {
                            TextButton(onClick = onSeries) { Text(stringResource(R.string.tr_repeat_sessions)) }
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
            when (part) {
                Part.SESSIONS -> FloatingActionButton(onClick = onNew) {
                    Icon(Icons.Filled.Add, contentDescription = stringResource(R.string.tr_new_session))
                }
                Part.CALENDAR -> FloatingActionButton(onClick = { onNewOn(day) }) {
                    Icon(Icons.Filled.Add, contentDescription = stringResource(R.string.tr_new_session))
                }
                Part.CYCLES -> FloatingActionButton(onClick = { onNewCycle(level) }) {
                    Icon(Icons.Filled.Add, contentDescription = stringResource(R.string.tr_new_cycle))
                }
                else -> {}
            }
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (part) {
            Part.CALENDAR -> CalendarTab(teamId, day, { day = it }, modifier, onSession, onMatch)
            Part.SESSIONS -> SessionsTab(teamId, modifier, onSession)
            Part.CYCLES -> CyclesTab(teamId, level, { level = it }, modifier, onCycle)
            Part.ATTENDANCE -> AttendanceTab(teamId, modifier)
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

@Composable
private fun Part.label(): String = when (this) {
    Part.CALENDAR -> stringResource(R.string.tr_calendar).replaceFirstChar { it.uppercase() }
    Part.SESSIONS -> stringResource(R.string.tr_sessions)
    Part.CYCLES -> stringResource(R.string.tr_programming)
    Part.ATTENDANCE -> stringResource(R.string.tr_attendance)
}

// ---- calendar

/** The month: a day to touch, a dot for each session and match; below,
 * what the selected day holds. */
@Composable
private fun CalendarTab(teamId: String, selected: LocalDate, onSelected: (LocalDate) -> Unit, modifier: Modifier, onSession: (String) -> Unit, onMatch: (String) -> Unit) {
    var month by rememberSaveable { mutableStateOf(YearMonth.from(selected)) }
    // the gyms are shared: every team, or one gym only
    var allTeams by rememberSaveable { mutableStateOf(false) }
    var venue by rememberSaveable { mutableStateOf<String?>(null) }
    val agenda by rememberAgenda(teamId.takeIf { !allTeams }, AgendaSpan.MONTH, month.atDay(1), month)
    val byDate = agenda.map { d -> d.copy(events = d.events.filter { venue == null || it.venue?.id == venue }) }.associateBy { it.date }
    val app = androidx.compose.ui.platform.LocalContext.current.applicationContext as Scout4AllApplication
    val venues by produceState(emptyList<io.github.naighes.scout4all.core.VenueRecord>()) {
        value = try {
            app.core { it.venues() }
        } catch (_: io.github.naighes.scout4all.core.MobileException) {
            emptyList()
        }
    }
    var picking by remember { mutableStateOf(false) }
    val today = LocalDate.now()
    LazyColumn(modifier, contentPadding = PaddingValues(bottom = 24.dp)) {
        item(key = "month") {
            Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(
                    month.format(DateTimeFormatter.ofPattern("LLLL yyyy", Locale.getDefault())).replaceFirstChar { it.uppercase() },
                    Modifier.weight(1f).title(),
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.primary,
                )
                IconButton(onClick = { month = month.minusMonths(1) }) {
                    Icon(Icons.AutoMirrored.Filled.KeyboardArrowLeft, contentDescription = stringResource(R.string.agenda_previous_week))
                }
                TextButton(onClick = {
                    month = YearMonth.now()
                    onSelected(today)
                }) { Text(stringResource(R.string.agenda_today)) }
                IconButton(onClick = { month = month.plusMonths(1) }) {
                    Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = stringResource(R.string.agenda_next_week))
                }
            }
        }
        item(key = "filters") {
            Row(Modifier.fillMaxWidth().padding(horizontal = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                androidx.compose.material3.FilterChip(
                    selected = allTeams,
                    onClick = { allTeams = !allTeams },
                    label = { Text(stringResource(if (allTeams) R.string.venue_all_teams else R.string.venue_this_team)) },
                )
                if (venues.isNotEmpty()) {
                    Box {
                        androidx.compose.material3.FilterChip(
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
        // the other teams' events are shown, not opened
        items(events, key = { "${it.teamId}/${it.start}/${it.id()}" }) { e ->
            EventRow(
                e,
                allTeams,
                { t, m -> if (t == teamId) onMatch(m) },
                { t, s -> if (t == teamId) onSession(s) },
            )
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
                        Box(Modifier.weight(1f).height(52.dp))
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
