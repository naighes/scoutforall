package io.github.naighes.scout4all

import androidx.compose.foundation.clickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavController
import androidx.navigation.NavGraphBuilder
import androidx.navigation.compose.composable
import androidx.navigation.NavType
import androidx.navigation.navArgument
import androidx.compose.ui.res.pluralStringResource
import io.github.naighes.scout4all.core.ChildRecord
import io.github.naighes.scout4all.core.CycleDraft
import io.github.naighes.scout4all.core.CycleRecord
import io.github.naighes.scout4all.core.DuplicateCycleDraft
import io.github.naighes.scout4all.core.Level
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.TrainingField
import io.github.naighes.scout4all.core.TrainingScope
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import kotlinx.coroutines.launch
import java.time.LocalDate

/** The form of a new cycle; from the sheet of a cycle, in it from a day. */
private const val NEW_CYCLE = "team/{id}/cycle-new/{level}?start={start}&parent={parent}&children={children}"

/** The form of a new cycle of `level` made from the free `children` (comma
 * separated: sessions for a microcycle, cycles of the level below). */
internal fun newCycleOf(teamId: String, level: Level, children: List<String>) =
    "team/$teamId/cycle-new/${level.name}?children=${children.joinToString(",")}"

/** The screens of a cycle: its data and totals, the form, what to add to
 * it, the copy of a microcycle. */
fun NavGraphBuilder.cycleRoutes(nav: NavController) {
    fun NavBackStackEntry.arg(name: String) = arguments?.getString(name).orEmpty()
    fun NavBackStackEntry.level() = runCatching { Level.valueOf(arg("level")) }.getOrDefault(Level.MICROCYCLE)
    fun NavController.back(entry: NavBackStackEntry) {
        if (currentBackStackEntry == entry) popBackStack()
    }
    composable("team/{id}/cycles/{level}/{cycle}") { entry ->
        val id = entry.arg("id")
        val level = entry.level()
        val cycle = entry.arg("cycle")
        CycleScreen(
            id, level, cycle,
            onBack = { nav.back(entry) },
            onEdit = { nav.navigate("team/$id/cycles/${level.name}/$cycle/edit") },
            onAdd = { nav.navigate("team/$id/cycles/${level.name}/$cycle/add") },
            onDuplicate = { nav.navigate("team/$id/cycles/${level.name}/$cycle/duplicate") },
            onChild = { child ->
                when (level) {
                    Level.PROGRAM -> nav.navigate("team/$id/cycles/${Level.MESOCYCLE.name}/$child")
                    Level.MESOCYCLE -> nav.navigate("team/$id/cycles/${Level.MICROCYCLE.name}/$child")
                    Level.MICROCYCLE -> nav.navigate("team/$id/training/$child")
                }
            },
            // a cycle of the level below in it, or a session on the day;
            // saved, the sheet comes back
            onNewChild = { day ->
                when (val down = level.child()) {
                    null -> nav.navigate("team/$id/cycle-session-new/$day")
                    else -> nav.navigate("team/$id/cycle-new/${down.name}?start=$day&parent=$cycle")
                }
            },
            onMatch = { match -> nav.navigate("team/$id/match/${android.net.Uri.encode(match)}") },
        )
    }
    composable(
        NEW_CYCLE,
        arguments = listOf(
            navArgument("start") { type = NavType.StringType; nullable = true; defaultValue = null },
            navArgument("parent") { type = NavType.StringType; nullable = true; defaultValue = null },
            navArgument("children") { type = NavType.StringType; nullable = true; defaultValue = null },
        ),
    ) { entry ->
        val id = entry.arg("id")
        val level = entry.level()
        val parent = entry.arguments?.getString("parent")
        val start = entry.arguments?.getString("start")
        val children = entry.arguments?.getString("children").orEmpty().split(',').filter { it.isNotBlank() }
        CycleFormScreen(id, level, null, onBack = { nav.back(entry) }, start = start, parent = parent, children = children, onSaved = { cycle ->
            when (parent) {
                null -> nav.navigate("team/$id/cycles/${level.name}/$cycle") { popUpTo(NEW_CYCLE) { inclusive = true } }
                else -> nav.back(entry)
            }
        })
    }
    composable("team/{id}/cycle-session-new/{day}") { entry ->
        SessionFormScreen(entry.arg("id"), null, onBack = { nav.back(entry) }, day = runCatching { LocalDate.parse(entry.arg("day")) }.getOrNull(), onSaved = { nav.back(entry) })
    }
    composable("team/{id}/cycles/{level}/{cycle}/edit") { entry ->
        CycleFormScreen(entry.arg("id"), entry.level(), entry.arg("cycle"), onBack = { nav.back(entry) }, onSaved = { nav.back(entry) })
    }
    composable("team/{id}/cycles/{level}/{cycle}/add") { entry ->
        AddChildrenScreen(entry.level(), entry.arg("cycle"), onBack = { nav.back(entry) }, onDone = { nav.back(entry) })
    }
    composable("team/{id}/cycles/{level}/{cycle}/duplicate") { entry ->
        DuplicateCycleScreen(entry.arg("cycle"), onBack = { nav.back(entry) }, onDone = { nav.back(entry) })
    }
}

/** The cycles of a level of the team, to open one. */
@Composable
fun CyclesTab(
    teamId: String,
    level: Level,
    onLevel: (Level) -> Unit,
    modifier: Modifier,
    onCycle: (Level, String) -> Unit,
    marked: List<String> = emptyList(),
    onMarked: (List<String>) -> Unit = {},
) {
    val cycles by rememberCore(Unit, level) { it.cycles(teamId, level) }
    // only the free ones are marked, to be put in a cycle of the level above
    val free = cycles?.filter { it.parentId == null }?.map { it.id }?.toSet()
    androidx.compose.runtime.LaunchedEffect(free) {
        if (free != null && marked.any { it !in free }) onMarked(marked.filter { it in free })
    }
    Column(modifier) {
        Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(Level.PROGRAM, Level.MESOCYCLE, Level.MICROCYCLE).forEach {
                FilterChip(selected = level == it, onClick = { onLevel(it) }, label = { Text(stringResource(it.plural())) })
            }
        }
        val all = cycles
        when (all) {
            null -> Centered(Modifier.fillMaxSize()) { CircularProgressIndicator() }
            else -> LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = fabRoom(marked.isNotEmpty()))) {
                // counted by name, none in words
                item(key = "count") {
                    Text(
                        countText(level, all.size),
                        Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                items(all.sortedByDescending { it.firstDay }, key = { it.id }) { c ->
                    val name = c.title.ifBlank { stringResource(level.label()) }
                    ListItem(
                        modifier = Modifier.clickable { onCycle(level, c.id) },
                        trailingContent = if (level.parent() != null && c.parentId == null) {
                            { MarkBox(name, c.id in marked) { onMarked(if (c.id in marked) marked - c.id else marked + c.id) } }
                        } else null,
                        overlineContent = { Text(windowText(c.firstDay, c.lastDay)) },
                        headlineContent = { Text(name) },
                    )
                }
                // what the levels are, for who has never planned a season:
                // under the list, so that the cycles come first (at a large
                // font it would push them below the fold)
                item(key = "intro") {
                    Text(
                        stringResource(R.string.tr_cycles_intro),
                        Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
        }
    }
}

/** The box that marks a free session or cycle, to make a cycle of them. */
@Composable
internal fun MarkBox(name: String, marked: Boolean, onToggle: () -> Unit) {
    val spoken = stringResource(R.string.tr_mark, name)
    androidx.compose.material3.Checkbox(
        checked = marked,
        onCheckedChange = { onToggle() },
        modifier = Modifier.semantics { contentDescription = spoken },
    )
}

private fun minutesText(minutes: UInt): String = "${minutes / 60u}:${(minutes % 60u).toString().padStart(2, '0')}"

/** What a cycle holds: the cycles of the level below, the sessions of a
 * microcycle (null). */
internal fun Level.child(): Level? = when (this) {
    Level.PROGRAM -> Level.MESOCYCLE
    Level.MESOCYCLE -> Level.MICROCYCLE
    Level.MICROCYCLE -> null
}

internal fun Level.createName(): Int = when (this) {
    Level.PROGRAM -> R.string.tr_create_program
    Level.MESOCYCLE -> R.string.tr_create_mesocycle
    Level.MICROCYCLE -> R.string.tr_create_microcycle
}

internal fun Level.editName(): Int = when (this) {
    Level.PROGRAM -> R.string.tr_edit_program
    Level.MESOCYCLE -> R.string.tr_edit_mesocycle
    Level.MICROCYCLE -> R.string.tr_edit_microcycle
}

/** What a cycle makes inside itself: a cycle of the level below, a session. */
private fun Level.newChildName(): Int = child()?.createName() ?: R.string.tr_new_session

/** What a cycle takes in among the free ones. */
private fun Level.addName(): Int = when (this) {
    Level.PROGRAM -> R.string.tr_add_mesocycles
    Level.MESOCYCLE -> R.string.tr_add_microcycles
    Level.MICROCYCLE -> R.string.tr_add_sessions
}

/** What a cycle says when it holds nothing yet. */
private fun Level.emptyName(): Int = when (this) {
    Level.PROGRAM -> R.string.tr_no_mesocycles
    Level.MESOCYCLE -> R.string.tr_no_microcycles
    Level.MICROCYCLE -> R.string.tr_no_sessions
}

/** [n] cycles of [of] (sessions when null) as the language counts them,
 * none in words: "nessuna seduta", not "0 sedute". */
@Composable
internal fun countText(of: Level?, n: Int): String {
    val (plural, zero) = when (of) {
        Level.PROGRAM -> R.plurals.tr_programs_count to R.string.tr_zero_programs
        Level.MESOCYCLE -> R.plurals.tr_mesocycles_count to R.string.tr_zero_mesocycles
        Level.MICROCYCLE -> R.plurals.tr_microcycles_count to R.string.tr_zero_microcycles
        null -> R.plurals.tr_sessions_count to R.string.tr_zero_sessions
    }
    return if (n == 0) stringResource(zero) else pluralStringResource(plural, n, n)
}

/** A cycle: its window and text, its sessions counted, what it holds named
 * by its level (a microcycle's sessions under their day, as the list of
 * the calendar), what it can make and take in. */
@OptIn(ExperimentalMaterial3Api::class, androidx.compose.foundation.layout.ExperimentalLayoutApi::class)
@Composable
fun CycleScreen(
    teamId: String,
    level: Level,
    cycleId: String,
    onBack: () -> Unit,
    onEdit: () -> Unit,
    onAdd: () -> Unit,
    onDuplicate: () -> Unit,
    onChild: (String) -> Unit,
    onNewChild: (LocalDate) -> Unit,
    onMatch: (String) -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    val cycle by rememberCore(Unit, cycleId) { it.cycle(level, cycleId) }
    val totals by rememberCore(Unit, cycleId) { it.trainingTotals(teamId, TrainingScope.Cycle(level, cycleId)) }
    val children by rememberCore(Unit, cycleId) { it.cycleChildren(level, cycleId) }
    val free by rememberCore(Unit, cycleId) { it.freeChildren(level, cycleId).size }
    var deleting by rememberSaveable { mutableStateOf(false) }
    var withChildren by rememberSaveable { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val first = cycle?.firstDay?.let { runCatching { LocalDate.parse(it) }.getOrNull() }
    val last = cycle?.lastDay?.let { runCatching { LocalDate.parse(it) }.getOrNull() }
    // a microcycle's days as the calendar reads them: its sessions and the
    // matches of the team
    val days by rememberAgendaBetween(teamId, first, last, level == Level.MICROCYCLE && first != null)
    val held = children.orEmpty().map { it.id }.toSet()
    val today = LocalDate.now()
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(cycle?.title?.ifBlank { null } ?: stringResource(level.label()), Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
                actions = {
                    if (level == Level.MICROCYCLE) {
                        TextButton(onClick = onDuplicate) { Text(stringResource(R.string.tr_duplicate_session)) }
                    }
                    IconButton(onClick = onEdit) {
                        Icon(Icons.Filled.Edit, contentDescription = stringResource(level.editName()))
                    }
                    DeleteIcon(stringResource(R.string.delete)) { deleting = true }
                },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        val c = cycle
        if (c == null || first == null || last == null) {
            Centered(modifier) { CircularProgressIndicator() }
            return@Scaffold
        }
        // a new cycle after the last one inside, or on the first day; a
        // session today when the microcycle holds it
        val next = when (level) {
            Level.MICROCYCLE -> today.takeIf { it in first..last } ?: first
            else -> children.orEmpty().mapNotNull { runCatching { LocalDate.parse(it.lastDay) }.getOrNull() }
                .maxOrNull()?.plusDays(1)?.takeIf { it <= last } ?: first
        }
        LazyColumn(modifier, contentPadding = PaddingValues(bottom = 24.dp)) {
            item(key = "data") {
                Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(6.dp)) {
                    Text(windowText(c.firstDay, c.lastDay), style = MaterialTheme.typography.titleMedium)
                    if (c.objectives.isNotBlank()) {
                        Text(stringResource(R.string.tr_objectives), style = MaterialTheme.typography.labelMedium)
                        Text(c.objectives)
                    }
                    if (c.notes.isNotBlank()) {
                        Text(stringResource(R.string.notes), style = MaterialTheme.typography.labelMedium)
                        Text(c.notes)
                    }
                }
            }
            totals?.let { t ->
                item(key = "totals") {
                    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                        TrainingSection(R.string.tr_sessions)
                        // the word, then the count: "2 completata" isn't Italian
                        Text(
                            listOf(
                                "${stringResource(R.string.tr_filter_planned)} ${t.planned}",
                                "${stringResource(R.string.tr_filter_completed)} ${t.completed}",
                                "${stringResource(R.string.tr_filter_cancelled)} ${t.cancelled}",
                            ).joinToString(" · "),
                        )
                        Text("${stringResource(R.string.tr_planned_time)}: ${minutesText(t.plannedMinutes)}")
                        Text("${stringResource(R.string.tr_actual_time)}: ${minutesText(t.actualMinutes)}")
                    }
                }
            }
            item(key = "held") {
                Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    // named by what it holds: microcycles, mesocycles; a
                    // microcycle's sessions are under the counts above,
                    // titled already
                    level.child()?.let { down ->
                        Text(
                            stringResource(down.plural()),
                            Modifier.title(),
                            style = MaterialTheme.typography.titleMedium,
                            color = MaterialTheme.colorScheme.primary,
                        )
                    }
                    if (children?.isEmpty() == true) {
                        Text(stringResource(level.emptyName()), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                    androidx.compose.foundation.layout.FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                        OutlinedButton(onClick = { onNewChild(next) }) { Text(stringResource(level.newChildName())) }
                        // the free ones only when there are some
                        if ((free ?: 0) > 0) {
                            OutlinedButton(onClick = onAdd) { Text(stringResource(level.addName())) }
                        }
                    }
                }
            }
            if (level == Level.MICROCYCLE) {
                days.forEach { d ->
                    val events = d.events.filter { e ->
                        when (val i = e.item) {
                            is io.github.naighes.scout4all.core.AgendaItemRecord.Session -> i.id in held
                            is io.github.naighes.scout4all.core.AgendaItemRecord.Match -> true
                        }
                    }
                    if (events.isNotEmpty()) {
                        val date = LocalDate.parse(d.date)
                        item(key = "day-${d.date}") { DayHeader(date, date == today) }
                        items(events, key = { "${d.date}/${it.start}/${it.id()}" }) { e ->
                            EventRow(e, false, { _, m -> onMatch(m) }, { _, s -> onChild(s) })
                        }
                    }
                }
            } else {
                items(children.orEmpty(), key = { it.id }) { child ->
                    ListItem(modifier = Modifier.clickable { onChild(child.id) }, headlineContent = { Text(windowText(child.firstDay, child.lastDay)) })
                }
            }
            error?.let { item(key = "error") { Text(it, Modifier.padding(16.dp), color = MaterialTheme.colorScheme.error) } }
        }
    }
    if (deleting) {
        val holds = children?.isNotEmpty() == true
        AlertDialog(
            onDismissRequest = { deleting = false },
            title = { Text(cycle?.title?.ifBlank { null } ?: stringResource(level.label())) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    // what it holds is asked only when it holds something
                    if (holds) {
                        Text(stringResource(R.string.tr_delete_cycle_keeps))
                        CheckRow(stringResource(R.string.tr_remove_with_children), withChildren, onChecked = { withChildren = it })
                    }
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    deleting = false
                    scope.launch {
                        try {
                            app.core { it.removeCycle(level, cycleId, holds && withChildren) }
                            onBack()
                        } catch (e: MobileException) {
                            error = e.failure().text
                        }
                    }
                }) { Text(stringResource(R.string.delete)) }
            },
            dismissButton = { TextButton(onClick = { deleting = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

/** The cycles of the level above that hold the day, and the one the cycle
 * has even if its window went out of it. */
@Composable
private fun rememberContaining(teamId: String, level: Level?, day: LocalDate?, current: String?): List<CycleRecord> {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val found by produceState(emptyList<CycleRecord>(), teamId, level, day, current) {
        value = try {
            if (level == null) emptyList() else app.core { core ->
                val holding = if (day == null) emptyList() else core.cyclesContaining(teamId, level, day.toString())
                val own = current?.takeIf { id -> holding.none { it.id == id } }
                    ?.let { id -> try { core.cycle(level, id) } catch (_: MobileException) { null } }
                holding + listOfNotNull(own)
            }
        } catch (_: MobileException) {
            emptyList()
        }
    }
    return found
}

internal fun Level.parent(): Level? = when (this) {
    Level.PROGRAM -> null
    Level.MESOCYCLE -> Level.PROGRAM
    Level.MICROCYCLE -> Level.MESOCYCLE
}

/** The form of a cycle, new (no `cycleId`) or edited. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CycleFormScreen(
    teamId: String,
    level: Level,
    cycleId: String?,
    onBack: () -> Unit,
    onSaved: (String) -> Unit,
    start: String? = null,
    parent: String? = null,
    children: List<String> = emptyList(),
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val draft by produceState<CycleDraft?>(null, cycleId) {
        value = try {
            app.core {
                when {
                    cycleId != null -> it.cycleDraft(level, cycleId)
                    // made from what was marked: the window that holds it
                    children.isNotEmpty() -> when (level) {
                        Level.MICROCYCLE -> it.proposedMicrocycle(children)
                        else -> it.proposedContainer(level, children)
                    } ?: it.newCycleDraft()
                    // made from the sheet of a cycle: in it, from the day given
                    else -> it.newCycleDraft().let { d -> d.copy(start = start ?: d.start, parent = parent ?: d.parent) }
                }
            }
        } catch (_: MobileException) {
            null
        }
    }
    var changed by rememberSaveable { mutableStateOf(false) }
    val back = rememberFormBack(changed, onBack)
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(if (cycleId == null) level.createName() else level.editName()), Modifier.title()) },
                navigationIcon = { BackButton(back) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        val d = draft
        if (d == null) Centered(modifier) { CircularProgressIndicator() }
        else CycleFields(teamId, level, cycleId, d, children.takeIf { cycleId == null }.orEmpty(), modifier, onChanged = { changed = true }, onSaved = onSaved)
    }
}

@Composable
private fun CycleFields(
    teamId: String,
    level: Level,
    cycleId: String?,
    draft: CycleDraft,
    children: List<String>,
    modifier: Modifier,
    onChanged: () -> Unit,
    onSaved: (String) -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    val first = remember {
        draft.start?.let { runCatching { LocalDate.parse(it) }.getOrNull() }
            ?: LocalDate.now().let { today ->
                if (level == Level.MICROCYCLE) today.minusDays((today.dayOfWeek.value - 1).toLong()) else today
            }
    }
    var title by rememberSaveable { mutableStateOf(draft.title) }
    var start by rememberSaveable { mutableStateOf<LocalDate?>(first) }
    var weeks by rememberSaveable { mutableStateOf(draft.weeks.ifBlank { if (level == Level.MICROCYCLE) "1" else "" }) }
    var end by rememberSaveable { mutableStateOf<LocalDate?>(draft.end?.let { runCatching { LocalDate.parse(it) }.getOrNull() }) }
    var objectives by rememberSaveable { mutableStateOf(draft.objectives) }
    var notes by rememberSaveable { mutableStateOf(draft.notes) }
    var parent by rememberSaveable { mutableStateOf(draft.parent) }
    var touched by rememberSaveable { mutableStateOf(false) }
    var failure by remember { mutableStateOf<Failure?>(null) }
    var outside by remember { mutableStateOf<String?>(null) }
    var busy by remember { mutableStateOf(false) }
    val options = rememberContaining(teamId, level.parent(), start, parent)
    val busyDays = rememberBusyDays(teamId)
    // a new cycle takes the one that holds its first day, until chosen otherwise
    androidx.compose.runtime.LaunchedEffect(options) {
        if (cycleId == null && !touched) parent = options.firstOrNull()?.id
    }
    fun edit(change: () -> Unit) {
        change()
        onChanged()
    }
    fun save(detach: Boolean) {
        busy = true
        scope.launch {
            failure = try {
                val saved = app.core {
                    it.saveCycle(
                        teamId, level, cycleId,
                        draft.copy(
                            title = title,
                            start = start?.toString(),
                            weeks = weeks,
                            end = end?.toString(),
                            objectives = objectives,
                            notes = notes,
                            parent = parent,
                        ),
                        children,
                        detach,
                    )
                }
                onSaved(saved.id)
                null
            } catch (e: MobileException.ChildrenOutside) {
                // what falls out of the new window: asked before detaching
                outside = e.reason
                null
            } catch (e: MobileException) {
                e.failure()
            }
            busy = false
        }
    }
    Column(modifier) {
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            OutlinedTextField(
                title,
                { edit { title = it } },
                Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.tr_session_title)) },
                singleLine = true,
            )
            DayField(stringResource(R.string.tr_cycle_start), start, onDay = { d -> edit { start = d } }, error = ErrorFor(failure, TrainingField.START), busy = busyDays)
            if (level == Level.PROGRAM) {
                DayField(stringResource(R.string.tr_cycle_end), end, onDay = { d -> edit { end = d } }, error = ErrorFor(failure, TrainingField.END), busy = busyDays)
            } else {
                NumberField(R.string.tr_cycle_weeks, weeks, ErrorFor(failure, TrainingField.WEEKS)) { edit { weeks = it } }
            }
            if (level.parent() != null && (options.isNotEmpty() || parent != null)) {
                ChoiceField(
                    stringResource(R.string.tr_cycle_parent),
                    listOf<Pair<String?, String>>(null to NO_CHOICE) + options.map { o -> o.id to listOf(o.title, windowText(o.firstDay, o.lastDay)).filter { it.isNotBlank() }.joinToString(" · ") },
                    parent,
                    onSelect = { id ->
                        edit {
                            touched = true
                            parent = id
                        }
                    },
                )
            }
            Lines(R.string.tr_objectives, objectives) { edit { objectives = it } }
            NotesField(notes) { edit { notes = it } }
        }
        failure.elsewhere(TrainingField.START, TrainingField.END, TrainingField.WEEKS)?.let {
            Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
        }
        Button(
            onClick = { save(false) },
            enabled = !busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
        ) { Text(stringResource(R.string.save)) }
    }
    outside?.let { reason ->
        AlertDialog(
            onDismissRequest = { outside = null },
            text = { Text(reason) },
            confirmButton = {
                TextButton(onClick = {
                    outside = null
                    save(true)
                }) { Text(stringResource(R.string.tr_detach_save)) }
            },
            dismissButton = { TextButton(onClick = { outside = null }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

/** The free cycles (or sessions) inside the window, to put in the cycle. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AddChildrenScreen(level: Level, cycleId: String, onBack: () -> Unit, onDone: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    val free by rememberCore(Unit, cycleId) { it.freeChildren(level, cycleId) }
    var chosen by rememberSaveable { mutableStateOf(emptyList<String>()) }
    var failure by remember { mutableStateOf<String?>(null) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(level.addName()), Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        val list = free
        when {
            list == null -> Centered(modifier) { CircularProgressIndicator() }
            list.isEmpty() -> Text(stringResource(R.string.tr_nothing_to_add), modifier.padding(16.dp))
            else -> Column(modifier) {
                LazyColumn(Modifier.weight(1f)) {
                    items(list, key = { it.id }) { child: ChildRecord ->
                        CheckRow(
                            windowText(child.firstDay, child.lastDay),
                            child.id in chosen,
                            onChecked = { on -> chosen = if (on) chosen + child.id else chosen - child.id },
                            modifier = Modifier.padding(horizontal = 16.dp),
                        )
                    }
                }
                failure?.let { Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error) }
                Button(
                    onClick = {
                        scope.launch {
                            try {
                                app.core { it.addChildren(level, cycleId, chosen) }
                                onDone()
                            } catch (e: MobileException) {
                                failure = e.failure().text
                            }
                        }
                    },
                    enabled = chosen.isNotEmpty(),
                    modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
                ) { Text(stringResource(R.string.tr_add_children)) }
            }
        }
    }
}

/** The copies of a microcycle: how many weeks later, how many times, what
 * goes with them. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DuplicateCycleScreen(cycleId: String, onBack: () -> Unit, onDone: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    var shift by rememberSaveable { mutableStateOf("1") }
    var copies by rememberSaveable { mutableStateOf("1") }
    var objectives by rememberSaveable { mutableStateOf(true) }
    var activities by rememberSaveable { mutableStateOf(true) }
    var failure by remember { mutableStateOf<Failure?>(null) }
    var free by remember { mutableIntStateOf(0) }
    var busy by remember { mutableStateOf(false) }
    var clashes by remember { mutableStateOf<List<io.github.naighes.scout4all.core.VenueClashRecord>?>(null) }
    fun duplicate(confirmed: Boolean) {
        val s = shift.trim().toUShortOrNull()
        val n = copies.trim().toUShortOrNull()
        if (s == null || n == null) {
            failure = Failure(TrainingField.WEEKS, "")
            return
        }
        busy = true
        scope.launch {
            failure = try {
                val draft = DuplicateCycleDraft(s, n, objectives, activities)
                val busyWith = if (confirmed) emptyList() else app.core { it.microcycleVenueClashes(cycleId, draft) }
                if (busyWith.isNotEmpty()) {
                    clashes = busyWith
                } else {
                    val made = app.core { it.duplicateMicrocycle(cycleId, draft) }
                    // copies out of the mesocycle are free: said
                    free = made.count { it.parentId == null }
                    if (free == 0) onDone()
                }
                null
            } catch (e: MobileException) {
                e.failure()
            }
            busy = false
        }
    }
    clashes?.let { ClashDialog(it, onConfirm = { clashes = null; duplicate(true) }, onDismiss = { clashes = null }) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.tr_duplicate_microcycle), Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).imePadding()) {
            Column(
                Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                NumberField(R.string.tr_shift_weeks, shift, ErrorFor(failure, TrainingField.WEEKS)) { shift = it }
                NumberField(R.string.tr_copies, copies, ErrorFor(failure, TrainingField.WEEKS)) { copies = it }
                CheckRow(stringResource(R.string.tr_copy_objectives), objectives, onChecked = { objectives = it })
                CheckRow(stringResource(R.string.tr_copy_activities), activities, onChecked = { activities = it })
            }
            failure.elsewhere(TrainingField.WEEKS)?.let {
                Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
            }
            Button(
                onClick = { duplicate(false) },
                enabled = !busy,
                modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
            ) { Text(stringResource(R.string.tr_duplicate_microcycle)) }
        }
    }
    if (free > 0) {
        AlertDialog(
            onDismissRequest = onDone,
            text = { Text(stringResource(R.string.tr_copies_free, free)) },
            confirmButton = { TextButton(onClick = onDone) { Text(stringResource(R.string.ok)) } },
        )
    }
}
