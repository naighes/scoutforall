package io.github.naighes.scout4all

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Delete
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.State
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.layout.imePadding
import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavController
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import io.github.naighes.scout4all.core.ActivityRecord
import io.github.naighes.scout4all.core.ActivityStatus
import io.github.naighes.scout4all.core.AttendanceSheetRecord
import io.github.naighes.scout4all.core.CycleRecord
import io.github.naighes.scout4all.core.Level
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Move
import io.github.naighes.scout4all.core.SeriesScope
import io.github.naighes.scout4all.core.SessionRecord
import io.github.naighes.scout4all.core.SessionStatus
import kotlinx.coroutines.launch
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

/** The screens of the training of a team: its sessions, a session with its
 * activities and attendance, the forms. */
fun NavGraphBuilder.trainingRoutes(nav: NavController) {
    fun NavBackStackEntry.arg(name: String) = arguments?.getString(name).orEmpty()
    fun NavController.back(entry: NavBackStackEntry) {
        if (currentBackStackEntry == entry) popBackStack()
    }
    composable("team/{id}/training") { entry ->
        val id = entry.arg("id")
        TrainingScreen(
            id,
            onBack = { nav.back(entry) },
            onSession = { nav.navigate("team/$id/training/$it") },
            onCycle = { level, cycle -> nav.navigate("team/$id/cycles/${level.name}/$cycle") },
            onNewCycle = { level -> nav.navigate("team/$id/cycle-new/${level.name}") },
            onNewOn = { day -> nav.navigate("team/$id/training-new-on/$day") },
            onMatch = { match -> nav.navigate("team/$id/match/${android.net.Uri.encode(match)}") },
            onVenues = { nav.navigate("venues") },
        )
    }
    composable("team/{id}/training-new-on/{day}") { entry ->
        val id = entry.arg("id")
        SessionFormScreen(id, null, onBack = { nav.back(entry) }, day = runCatching { LocalDate.parse(entry.arg("day")) }.getOrNull(), onRepeated = { nav.back(entry) }, onSaved = { session ->
            nav.navigate("team/$id/training/$session") { popUpTo("team/{id}/training-new-on/{day}") { inclusive = true } }
        })
    }
    cycleRoutes(nav)
    composable("team/{id}/training-new") { entry ->
        val id = entry.arg("id")
        SessionFormScreen(id, null, onBack = { nav.back(entry) }, onSaved = { session ->
            nav.navigate("team/$id/training/$session") { popUpTo("team/{id}/training-new") { inclusive = true } }
        })
    }
    composable("team/{id}/training/{session}") { entry ->
        val id = entry.arg("id")
        val session = entry.arg("session")
        SessionScreen(
            id,
            session,
            onBack = { nav.back(entry) },
            onEdit = { nav.navigate("team/$id/training/$session/edit") },
            onActivity = { activity -> nav.navigate("team/$id/training/$session/activity/${activity ?: "new"}") },
            onAttendance = { nav.navigate("team/$id/training/$session/attendance") },
            onDuplicate = { nav.navigate("team/$id/training/$session/duplicate") },
        )
    }
    composable("team/{id}/training/{session}/duplicate") { entry ->
        val id = entry.arg("id")
        DuplicateScreen(id, entry.arg("session"), onBack = { nav.back(entry) }, onDuplicated = { copy ->
            nav.navigate("team/$id/training/$copy") { popUpTo("team/{id}/training/{session}/duplicate") { inclusive = true } }
        })
    }
    composable("team/{id}/training/{session}/edit") { entry ->
        val id = entry.arg("id")
        SessionFormScreen(id, entry.arg("session"), onBack = { nav.back(entry) }, onSaved = { nav.back(entry) })
    }
    composable("team/{id}/training/{session}/activity/{activity}") { entry ->
        val activity = entry.arg("activity")
        ActivityFormScreen(
            entry.arg("id"),
            entry.arg("session"),
            activity.takeIf { it != "new" },
            onBack = { nav.back(entry) },
            onDone = { nav.back(entry) },
        )
    }
    composable("team/{id}/training/{session}/attendance") { entry ->
        AttendanceScreen(entry.arg("id"), entry.arg("session"), onBack = { nav.back(entry) })
    }
}

/** What a session is called: its title, or "Training". */
@Composable
fun SessionRecord.name(): String = title.ifBlank { stringResource(R.string.agenda_training) }

/** A moment as the phone writes it, from the core's text. */
fun momentText(start: String): String = try {
    LocalDateTime.parse(start).format(DateTimeFormatter.ofLocalizedDateTime(FormatStyle.MEDIUM, FormatStyle.SHORT))
} catch (_: java.time.DateTimeException) {
    start
}

@Composable
private fun SessionStatus.label(): String = stringResource(
    when (this) {
        SessionStatus.PLANNED -> R.string.tr_status_planned
        SessionStatus.COMPLETED -> R.string.tr_status_completed
        SessionStatus.CANCELLED -> R.string.agenda_cancelled
    },
)

/** The sessions of the team read from the core, again when `refresh`
 * changes; null until read. */
@Composable
internal fun <T> rememberCore(refresh: Any?, key: Any?, read: (io.github.naighes.scout4all.core.Scout4All) -> T): State<T?> {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    return produceState<T?>(null, refresh, key) {
        value = try {
            app.core(read)
        } catch (_: MobileException) {
            null
        }
    }
}

/** A session: its data, its state, its activities in order, the attendance,
 * and what can be done with it. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SessionScreen(
    teamId: String,
    sessionId: String,
    onBack: () -> Unit,
    onEdit: () -> Unit,
    onActivity: (String?) -> Unit,
    onAttendance: () -> Unit,
    onDuplicate: () -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    var refresh by remember { mutableIntStateOf(0) }
    val session by rememberCore(refresh, sessionId) { it.trainingSession(sessionId) }
    val activities by rememberCore(refresh, sessionId) { it.sessionActivities(sessionId) }
    val attendance by rememberCore(refresh, sessionId) { it.sessionAttendance(sessionId) }
    var error by rememberSaveable { mutableStateOf<String?>(null) }
    var completing by rememberSaveable { mutableStateOf(false) }
    var deleting by rememberSaveable { mutableStateOf(false) }
    // in a series with other planned sessions: which ones go
    var deletingSeries by rememberSaveable { mutableStateOf(false) }
    var removing by rememberSaveable { mutableStateOf<String?>(null) }
    fun act(call: (io.github.naighes.scout4all.core.Scout4All) -> Unit) {
        scope.launch {
            try {
                app.core(call)
                refresh++
            } catch (e: MobileException) {
                error = e.reason
            }
        }
    }
    Deletion(
        stringResource(R.string.tr_delete_session_text),
        asking = deleting,
        onAsking = { deleting = it },
        delete = { it.removeSession(sessionId) },
        onDeleted = onBack,
    )
    if (deletingSeries) {
        ScopeDialog(
            stringResource(R.string.tr_scope_delete_text),
            stringResource(R.string.delete),
            deleting = true,
            onChosen = { range ->
                deletingSeries = false
                scope.launch {
                    try {
                        app.core {
                            if (range == io.github.naighes.scout4all.core.SeriesScope.THIS) it.removeSession(sessionId)
                            else it.removeSeries(sessionId, range)
                        }
                        onBack()
                    } catch (e: MobileException) {
                        error = e.reason
                    }
                }
            },
            onDismiss = { deletingSeries = false },
        )
    }
    // the question of the series, or the usual one
    fun askDeletion() {
        scope.launch {
            val series = try {
                app.core { it.asksSeriesScope(sessionId, null) }
            } catch (_: MobileException) {
                false
            }
            if (series) deletingSeries = true else deleting = true
        }
    }
    removing?.let { activity ->
        Deletion(
            stringResource(R.string.tr_delete_activity_text),
            asking = true,
            onAsking = { if (!it) removing = null },
            delete = { it.removeActivity(activity) },
            onDeleted = {
                removing = null
                refresh++
            },
        )
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.agenda_training), Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
                actions = {
                    if (session != null) {
                        IconButton(onClick = onEdit) {
                            Icon(Icons.Filled.Edit, contentDescription = stringResource(R.string.tr_edit_session))
                        }
                        DeleteIcon(stringResource(R.string.delete)) { askDeletion() }
                    }
                },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        val s = session
        if (s == null) {
            Centered(modifier) { CircularProgressIndicator() }
            return@Scaffold
        }
        LazyColumn(modifier, contentPadding = PaddingValues(bottom = 24.dp)) {
            item(key = "data") { SessionData(s, attendance) }
            item(key = "state") {
                Row(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    when (s.status) {
                        SessionStatus.PLANNED -> {
                            Button(onClick = {
                                // the planned activities are asked about
                                if (activities.orEmpty().any { it.status == ActivityStatus.PLANNED }) completing = true
                                else act { it.completeSession(sessionId, emptyList()) }
                            }) { Text(stringResource(R.string.tr_complete_session)) }
                            OutlinedButton(onClick = { act { it.setSessionStatus(sessionId, SessionStatus.CANCELLED) } }) {
                                Text(stringResource(R.string.tr_cancel_session))
                            }
                        }
                        else -> OutlinedButton(onClick = { act { it.setSessionStatus(sessionId, SessionStatus.PLANNED) } }) {
                            Text(stringResource(R.string.tr_reopen_session))
                        }
                    }
                }
            }
            error?.let { item(key = "error") { Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error) } }
            item(key = "activities") {
                Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp, top = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        stringResource(R.string.tr_activities),
                        Modifier.weight(1f).title(),
                        style = MaterialTheme.typography.titleSmall,
                        color = MaterialTheme.colorScheme.primary,
                    )
                    IconButton(onClick = { onActivity(null) }) {
                        Icon(Icons.Filled.Add, contentDescription = stringResource(R.string.tr_new_activity))
                    }
                }
            }
            val list = activities.orEmpty()
            if (list.isEmpty()) {
                item(key = "no-activities") { Text(stringResource(R.string.tr_no_activities), Modifier.padding(horizontal = 16.dp, vertical = 8.dp)) }
            }
            items(list, key = { it.id }) { a ->
                val at = list.indexOf(a)
                ActivityRow(
                    a,
                    first = at == 0,
                    last = at == list.lastIndex,
                    onClick = { onActivity(a.id) },
                    onMove = { to -> act { it.moveActivity(a.id, to) } },
                )
            }
            item(key = "more") {
                Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    FilledTonalButton(onClick = onAttendance, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(R.string.tr_attendance))
                    }
                    OutlinedButton(onClick = onDuplicate, modifier = Modifier.fillMaxWidth()) {
                        Text(stringResource(R.string.tr_duplicate_session))
                    }
                }
            }
        }
        if (completing) {
            CompleteDialog(
                activities.orEmpty().filter { it.status == ActivityStatus.PLANNED },
                onDismiss = { completing = false },
                onComplete = { ids ->
                    completing = false
                    act { it.completeSession(sessionId, ids) }
                },
            )
        }
    }
}

@Composable
internal fun DeleteIcon(description: String, onClick: () -> Unit) = IconButton(onClick = onClick) {
    Icon(Icons.Filled.Delete, contentDescription = description)
}

@Composable
private fun SessionData(s: SessionRecord, attendance: AttendanceSheetRecord?) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(s.name(), style = MaterialTheme.typography.titleLarge)
        Text(momentText(s.start), style = MaterialTheme.typography.titleMedium)
        val minutes = s.actualDurationMin?.let { "${s.durationMin} → $it" } ?: s.durationMin.toString()
        Text(
            listOfNotNull(
                "$minutes ${stringResource(R.string.tr_min_short)}",
                venueLabel(s.venueId),
            ).joinToString(" · "),
        )
        val state = when {
            s.toClose -> stringResource(R.string.agenda_to_close)
            s.status != SessionStatus.PLANNED -> s.status.label()
            else -> null
        }
        state?.let { Text(it, color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.SemiBold) }
        if (s.status == SessionStatus.COMPLETED && attendance?.recorded == false) {
            Text(stringResource(R.string.tr_attendance_not_recorded), color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Labelled(R.string.tr_objectives, s.objectives)
        Labelled(R.string.notes, s.notes)
        Labelled(R.string.tr_final_remarks, s.finalRemarks)
    }
}

@Composable
private fun Labelled(label: Int, text: String) {
    if (text.isBlank()) return
    Column {
        Text(stringResource(label), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(text)
    }
}

@Composable
private fun ActivityRow(a: ActivityRecord, first: Boolean, last: Boolean, onClick: () -> Unit, onMove: (Move) -> Unit) {
    ListItem(
        modifier = Modifier.clickable(onClick = onClick),
        headlineContent = {
            Text(a.name, textDecoration = if (a.status == ActivityStatus.SKIPPED) TextDecoration.LineThrough else null)
        },
        supportingContent = {
            val minutes = a.plannedMin?.let { p -> a.actualMin?.let { "$p → $it" } ?: "$p" } ?: a.actualMin?.toString()
            val state = when (a.status) {
                ActivityStatus.PLANNED -> null
                ActivityStatus.COMPLETED -> stringResource(R.string.tr_status_completed)
                ActivityStatus.SKIPPED -> stringResource(R.string.tr_status_skipped)
            }
            val line = listOfNotNull(minutes?.let { "$it ${stringResource(R.string.tr_min_short)}" }, state)
                .joinToString(" · ")
            if (line.isNotEmpty()) Text(line)
        },
        trailingContent = {
            Row {
                IconButton(onClick = { onMove(Move.UP) }, enabled = !first) {
                    Icon(Icons.Filled.KeyboardArrowUp, contentDescription = stringResource(R.string.tr_move_up))
                }
                IconButton(onClick = { onMove(Move.DOWN) }, enabled = !last) {
                    Icon(Icons.Filled.KeyboardArrowDown, contentDescription = stringResource(R.string.tr_move_down))
                }
            }
        },
    )
}

/** The planned activities to mark completed with the session, all
 * checked. */
@Composable
private fun CompleteDialog(planned: List<ActivityRecord>, onDismiss: () -> Unit, onComplete: (List<String>) -> Unit) {
    var unchecked by rememberSaveable { mutableStateOf(setOf<String>()) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.tr_complete_session)) },
        text = {
            Column {
                Text(stringResource(R.string.tr_complete_activities))
                planned.forEach { a ->
                    CheckRow(a.name, a.id !in unchecked, onChecked = { on ->
                        unchecked = if (on) unchecked - a.id else unchecked + a.id
                    })
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { onComplete(planned.map { it.id }.filter { it !in unchecked }) }) {
                Text(stringResource(R.string.tr_complete_session))
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

val HOUR_MINUTE: DateTimeFormatter = DateTimeFormatter.ofPattern("HH:mm")

@Composable
private fun Modifier.verticalScrollable(): Modifier = this.then(Modifier.verticalScroll(rememberScrollState()))

/** The microcycles of the team that hold the day, and the one the session
 * has even if the day went out of it. */
@Composable
fun rememberMicrocycles(teamId: String, day: LocalDate, current: String?): List<CycleRecord> {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val found by produceState(emptyList<CycleRecord>(), teamId, day, current) {
        value = try {
            app.core { core ->
                val holding = core.cyclesContaining(teamId, Level.MICROCYCLE, day.toString())
                val own = current?.takeIf { id -> holding.none { it.id == id } }
                    ?.let { id -> try { core.cycle(Level.MICROCYCLE, id) } catch (_: MobileException) { null } }
                holding + listOfNotNull(own)
            }
        } catch (_: MobileException) {
            emptyList()
        }
    }
    return found
}

/** The microcycle of a session: one of those that hold its day, or none. */
@Composable
fun MicrocycleChoice(options: List<CycleRecord>, selected: String?, onSelect: (String?) -> Unit) {
    if (options.isEmpty() && selected == null) return
    ChoiceField(
        stringResource(R.string.tr_microcycle),
        listOf<Pair<String?, String>>(null to NO_CHOICE) + options.map { o -> o.id to listOf(o.title, windowText(o.firstDay, o.lastDay)).filter { it.isNotBlank() }.joinToString(" · ") },
        selected,
        onSelect = onSelect,
    )
}

