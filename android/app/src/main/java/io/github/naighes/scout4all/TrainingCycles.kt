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
import io.github.naighes.scout4all.core.ChildRecord
import io.github.naighes.scout4all.core.CycleDraft
import io.github.naighes.scout4all.core.CycleRecord
import io.github.naighes.scout4all.core.DuplicateCycleDraft
import io.github.naighes.scout4all.core.Level
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.TrainingField
import io.github.naighes.scout4all.core.TrainingScope
import kotlinx.coroutines.launch
import java.time.LocalDate

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
        )
    }
    composable("team/{id}/cycle-new/{level}") { entry ->
        val id = entry.arg("id")
        val level = entry.level()
        CycleFormScreen(id, level, null, onBack = { nav.back(entry) }, onSaved = { cycle ->
            nav.navigate("team/$id/cycles/${level.name}/$cycle") { popUpTo("team/{id}/cycle-new/{level}") { inclusive = true } }
        })
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
fun CyclesTab(teamId: String, level: Level, onLevel: (Level) -> Unit, modifier: Modifier, onCycle: (Level, String) -> Unit) {
    val cycles by rememberCore(Unit, level) { it.cycles(teamId, level) }
    Column(modifier) {
        Row(Modifier.horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            listOf(Level.PROGRAM, Level.MESOCYCLE, Level.MICROCYCLE).forEach {
                FilterChip(selected = level == it, onClick = { onLevel(it) }, label = { Text(stringResource(it.plural())) })
            }
        }
        val all = cycles
        when (all) {
            null -> Centered(Modifier.fillMaxSize()) { CircularProgressIndicator() }
            else -> LazyColumn(Modifier.fillMaxSize(), contentPadding = PaddingValues(bottom = 88.dp)) {
                // what the levels are, for who has never planned a season:
                // it scrolls away with the list
                item(key = "intro") {
                    Text(
                        stringResource(R.string.tr_cycles_intro),
                        Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (all.isEmpty()) {
                    item(key = "none") { Text(stringResource(R.string.tr_no_cycles), Modifier.padding(16.dp)) }
                }
                items(all.sortedByDescending { it.firstDay }, key = { it.id }) { c ->
                    ListItem(
                        modifier = Modifier.clickable { onCycle(level, c.id) },
                        overlineContent = { Text(windowText(c.firstDay, c.lastDay)) },
                        headlineContent = { Text(c.title.ifBlank { stringResource(level.label()) }) },
                    )
                }
            }
        }
    }
}

private fun minutesText(minutes: UInt): String = "${minutes / 60u}:${(minutes % 60u).toString().padStart(2, '0')}"

/** A cycle: its window and text, its totals, what it holds. */
@OptIn(ExperimentalMaterial3Api::class)
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
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    val cycle by rememberCore(Unit, cycleId) { it.cycle(level, cycleId) }
    val totals by rememberCore(Unit, cycleId) { it.trainingTotals(teamId, TrainingScope.Cycle(level, cycleId)) }
    val children by rememberCore(Unit, cycleId) { it.cycleChildren(level, cycleId) }
    var deleting by rememberSaveable { mutableStateOf(false) }
    var withChildren by rememberSaveable { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
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
                        Icon(Icons.Filled.Edit, contentDescription = stringResource(R.string.tr_edit_cycle))
                    }
                    DeleteIcon(stringResource(R.string.delete_forever)) { deleting = true }
                },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        val c = cycle
        if (c == null) {
            Centered(modifier) { CircularProgressIndicator() }
            return@Scaffold
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
                        TrainingSection(R.string.tr_cycle_totals)
                        Text(
                            listOf(
                                "${t.planned} ${stringResource(R.string.tr_status_planned)}",
                                "${t.completed} ${stringResource(R.string.tr_status_completed)}",
                                "${t.cancelled} ${stringResource(R.string.agenda_cancelled)}",
                            ).joinToString(" · "),
                        )
                        Text("${stringResource(R.string.tr_planned_time)}: ${minutesText(t.plannedMinutes)}")
                        Text("${stringResource(R.string.tr_actual_time)}: ${minutesText(t.actualMinutes)}")
                    }
                }
            }
            item(key = "contains") {
                Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp, top = 8.dp), verticalAlignment = androidx.compose.ui.Alignment.CenterVertically) {
                    Text(stringResource(R.string.tr_cycle_contains), Modifier.weight(1f).title(), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.primary)
                    TextButton(onClick = onAdd) { Text(stringResource(R.string.tr_add_children)) }
                }
            }
            items(children.orEmpty(), key = { it.id }) { child ->
                ListItem(modifier = Modifier.clickable { onChild(child.id) }, headlineContent = { Text(windowText(child.firstDay, child.lastDay)) })
            }
            error?.let { item(key = "error") { Text(it, Modifier.padding(16.dp), color = MaterialTheme.colorScheme.error) } }
        }
    }
    if (deleting) {
        AlertDialog(
            onDismissRequest = { deleting = false },
            title = { Text(cycle?.title?.ifBlank { null } ?: stringResource(level.label())) },
            text = { CheckRow(stringResource(R.string.tr_remove_with_children), withChildren, onChecked = { withChildren = it }) },
            confirmButton = {
                TextButton(onClick = {
                    deleting = false
                    scope.launch {
                        try {
                            app.core { it.removeCycle(level, cycleId, withChildren) }
                            onBack()
                        } catch (e: MobileException) {
                            error = e.failure().text
                        }
                    }
                }) { Text(stringResource(R.string.delete_forever)) }
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

private fun Level.parent(): Level? = when (this) {
    Level.PROGRAM -> null
    Level.MESOCYCLE -> Level.PROGRAM
    Level.MICROCYCLE -> Level.MESOCYCLE
}

/** The form of a cycle, new (no `cycleId`) or edited. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CycleFormScreen(teamId: String, level: Level, cycleId: String?, onBack: () -> Unit, onSaved: (String) -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val draft by produceState<CycleDraft?>(null, cycleId) {
        value = try {
            app.core { if (cycleId == null) it.newCycleDraft() else it.cycleDraft(level, cycleId) }
        } catch (_: MobileException) {
            null
        }
    }
    var changed by rememberSaveable { mutableStateOf(false) }
    val back = rememberFormBack(changed, onBack)
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(if (cycleId == null) R.string.tr_new_cycle else R.string.tr_edit_cycle), Modifier.title()) },
                navigationIcon = { BackButton(back) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        val d = draft
        if (d == null) Centered(modifier) { CircularProgressIndicator() }
        else CycleFields(teamId, level, cycleId, d, modifier, onChanged = { changed = true }, onSaved = onSaved)
    }
}

@Composable
private fun CycleFields(
    teamId: String,
    level: Level,
    cycleId: String?,
    draft: CycleDraft,
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
                        emptyList(),
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
            DayField(stringResource(R.string.tr_cycle_start), start, onDay = { d -> edit { start = d } }, error = ErrorFor(failure, TrainingField.START))
            if (level == Level.PROGRAM) {
                DayField(stringResource(R.string.tr_cycle_end), end, onDay = { d -> edit { end = d } }, error = ErrorFor(failure, TrainingField.END))
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
                title = { Text(stringResource(R.string.tr_add_children), Modifier.title()) },
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
