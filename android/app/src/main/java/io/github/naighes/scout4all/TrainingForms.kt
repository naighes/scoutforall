package io.github.naighes.scout4all

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.ActivityDraft
import io.github.naighes.scout4all.core.ActivityHitRecord
import io.github.naighes.scout4all.core.ActivityStatus
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.SeriesBand
import io.github.naighes.scout4all.core.SeriesConflictRecord
import io.github.naighes.scout4all.core.SeriesDraft
import io.github.naighes.scout4all.core.SessionDraft
import io.github.naighes.scout4all.core.TrainingField
import kotlinx.coroutines.launch
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime
import java.time.format.TextStyle
import java.util.Locale

/** The text under the field an error is about, or at the bottom. */
private class Failure(val field: TrainingField?, val text: String)

private fun MobileException.failure() = Failure((this as? MobileException.InvalidTraining)?.field, reason)

private fun ErrorFor(failure: Failure?, vararg fields: TrainingField): String? =
    failure?.takeIf { it.field in fields }?.text

private fun Failure?.elsewhere(vararg fields: TrainingField): String? =
    this?.takeIf { it.field == null || it.field !in fields }?.text

@Composable
private fun Section(title: Int) = Text(
    stringResource(title),
    Modifier.title(),
    style = MaterialTheme.typography.titleMedium,
    color = MaterialTheme.colorScheme.primary,
)

/** The form of a session, new (no `sessionId`) or edited. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SessionFormScreen(teamId: String, sessionId: String?, onBack: () -> Unit, onSaved: (String) -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val draft by produceState<SessionDraft?>(null, sessionId) {
        value = try {
            app.core { if (sessionId == null) it.newSessionDraft() else it.sessionDraft(sessionId) }
        } catch (_: MobileException) {
            null
        }
    }
    val places by produceState(emptyList<String>(), teamId) {
        value = try {
            app.core { it.trainingLocations(teamId) }
        } catch (_: MobileException) {
            emptyList()
        }
    }
    var changed by rememberSaveable { mutableStateOf(false) }
    val back = rememberFormBack(changed, onBack)
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        stringResource(if (sessionId == null) R.string.tr_new_session else R.string.tr_edit_session),
                        Modifier.title(),
                    )
                },
                navigationIcon = { BackButton(back) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        val d = draft
        if (d == null) Centered(modifier) { CircularProgressIndicator() }
        else SessionFields(teamId, sessionId, d, places, modifier, onChanged = { changed = true }, onSaved = onSaved)
    }
}

@Composable
private fun SessionFields(
    teamId: String,
    sessionId: String?,
    draft: SessionDraft,
    places: List<String>,
    modifier: Modifier,
    onChanged: () -> Unit,
    onSaved: (String) -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    // a new session: in the next hour; the form of the core has no day
    val first = remember {
        draft.start?.let(LocalDateTime::parse)
            ?: LocalDateTime.now().plusHours(1).withMinute(0).withSecond(0).withNano(0)
    }
    var day by rememberSaveable { mutableStateOf<LocalDate?>(first.toLocalDate()) }
    var time by rememberSaveable { mutableStateOf<LocalTime?>(first.toLocalTime()) }
    var duration by rememberSaveable { mutableStateOf(draft.duration) }
    var actual by rememberSaveable { mutableStateOf(draft.actualDuration) }
    var location by rememberSaveable { mutableStateOf(draft.location) }
    var title by rememberSaveable { mutableStateOf(draft.title) }
    var objectives by rememberSaveable { mutableStateOf(draft.objectives) }
    var notes by rememberSaveable { mutableStateOf(draft.notes) }
    var remarks by rememberSaveable { mutableStateOf(draft.finalRemarks) }
    var micro by rememberSaveable { mutableStateOf(draft.microcycle) }
    var touched by rememberSaveable { mutableStateOf(false) }
    var failure by remember { mutableStateOf<Failure?>(null) }
    var busy by remember { mutableStateOf(false) }
    val options = rememberMicrocycles(teamId, day ?: first.toLocalDate(), micro)
    // a new session takes the microcycle of its day, until chosen otherwise
    LaunchedEffect(options) {
        if (sessionId == null && !touched) micro = options.firstOrNull()?.id
    }
    fun edit(change: () -> Unit) {
        change()
        onChanged()
    }
    Column(modifier) {
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            DayField(
                stringResource(R.string.date),
                day,
                onDay = { d -> edit { day = d } },
                error = ErrorFor(failure, TrainingField.START),
            )
            TimeField(stringResource(R.string.tr_time), time, onTime = { t -> edit { time = t } })
            NumberField(R.string.tr_session_duration, duration, ErrorFor(failure, TrainingField.DURATION)) { edit { duration = it } }
            SuggestField(
                location,
                stringResource(R.string.tr_session_location),
                places,
                error = null,
                onChange = { edit { location = it } },
                onPick = { edit { location = it } },
            )
            OutlinedTextField(
                title,
                { edit { title = it } },
                Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.tr_session_title)) },
                singleLine = true,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Next),
            )
            MicrocycleChoice(options, micro) { id ->
                edit {
                    touched = true
                    micro = id
                }
            }
            Lines(R.string.tr_objectives, objectives) { edit { objectives = it } }
            NotesField(notes) { edit { notes = it } }
            Lines(R.string.tr_final_remarks, remarks) { edit { remarks = it } }
            NumberField(R.string.tr_session_actual, actual, ErrorFor(failure, TrainingField.ACTUAL_DURATION)) { edit { actual = it } }
        }
        failure.elsewhere(TrainingField.START, TrainingField.DURATION, TrainingField.ACTUAL_DURATION)?.let {
            Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
        }
        // big and at the bottom, where the thumb is
        Button(
            onClick = {
                busy = true
                scope.launch {
                    failure = try {
                        val start = day?.let { d -> time?.let { t -> "${d}T${t.format(HOUR_MINUTE)}" } }
                        val saved = app.core {
                            it.saveSession(
                                teamId,
                                sessionId,
                                draft.copy(
                                    start = start,
                                    duration = duration,
                                    actualDuration = actual,
                                    location = location,
                                    title = title,
                                    objectives = objectives,
                                    notes = notes,
                                    finalRemarks = remarks,
                                    microcycle = micro,
                                ),
                            )
                        }
                        onSaved(saved.id)
                        null
                    } catch (e: MobileException) {
                        e.failure()
                    }
                    busy = false
                }
            },
            enabled = !busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
        ) { Text(stringResource(R.string.save)) }
    }
}

@Composable
private fun NumberField(label: Int, value: String, error: String?, onChange: (String) -> Unit) = OutlinedTextField(
    value,
    onChange,
    Modifier.fillMaxWidth(),
    label = { Text(stringResource(label)) },
    singleLine = true,
    isError = error != null,
    supportingText = error?.let { { Text(it) } },
    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Next),
)

@Composable
private fun Lines(label: Int, value: String, onChange: (String) -> Unit) = OutlinedTextField(
    value,
    onChange,
    Modifier.fillMaxWidth(),
    label = { Text(stringResource(label)) },
    minLines = 2,
    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
)

/** An activity of a session, new (no `activityId`) or edited. A new one
 * may copy one written before. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ActivityFormScreen(teamId: String, sessionId: String, activityId: String?, onBack: () -> Unit, onDone: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    var copied by remember { mutableStateOf<ActivityDraft?>(null) }
    val draft by produceState<ActivityDraft?>(null, activityId) {
        value = try {
            app.core { if (activityId == null) it.newActivityDraft() else it.activityDraft(sessionId, activityId) }
        } catch (_: MobileException) {
            null
        }
    }
    var changed by rememberSaveable { mutableStateOf(false) }
    val back = rememberFormBack(changed, onBack)
    var deleting by rememberSaveable { mutableStateOf(false) }
    Deletion(
        stringResource(R.string.tr_delete_activity_text),
        asking = deleting,
        onAsking = { deleting = it },
        delete = { core -> activityId?.let(core::removeActivity) },
        onDeleted = onDone,
    )
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        stringResource(if (activityId == null) R.string.tr_new_activity else R.string.tr_edit_activity),
                        Modifier.title(),
                    )
                },
                navigationIcon = { BackButton(back) },
                actions = {
                    if (activityId != null) {
                        IconButton(onClick = { deleting = true }) {
                            Icon(Icons.Filled.Delete, contentDescription = stringResource(R.string.delete_forever))
                        }
                    }
                },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        val d = copied ?: draft
        if (d == null) Centered(modifier) { CircularProgressIndicator() }
        else ActivityFields(
            teamId,
            sessionId,
            activityId,
            d,
            modifier,
            // a copy is a new draft: the fields start again from it
            copying = activityId == null,
            onCopy = {
                copied = it
                changed = true
            },
            onChanged = { changed = true },
            onDone = onDone,
        )
    }
}

@Composable
private fun ActivityFields(
    teamId: String,
    sessionId: String,
    activityId: String?,
    draft: ActivityDraft,
    modifier: Modifier,
    copying: Boolean,
    onCopy: (ActivityDraft) -> Unit,
    onChanged: () -> Unit,
    onDone: () -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    var name by rememberSaveable(draft) { mutableStateOf(draft.name) }
    var description by rememberSaveable(draft) { mutableStateOf(draft.description) }
    var planned by rememberSaveable(draft) { mutableStateOf(draft.planned) }
    var actual by rememberSaveable(draft) { mutableStateOf(draft.actual) }
    var notes by rememberSaveable(draft) { mutableStateOf(draft.notes) }
    var status by rememberSaveable(draft) { mutableStateOf(draft.status) }
    var failure by remember { mutableStateOf<Failure?>(null) }
    var busy by remember { mutableStateOf(false) }
    fun edit(change: () -> Unit) {
        change()
        onChanged()
    }
    Column(modifier) {
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            if (copying) CopyActivity(teamId, onCopy)
            OutlinedTextField(
                name,
                { edit { name = it } },
                Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.name)) },
                singleLine = true,
                isError = ErrorFor(failure, TrainingField.NAME) != null,
                supportingText = ErrorFor(failure, TrainingField.NAME)?.let { { Text(it) } },
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Next),
            )
            OutlinedTextField(
                description,
                { edit { description = it } },
                Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.tr_activity_description)) },
                minLines = 2,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
            )
            NumberField(R.string.tr_activity_planned, planned, ErrorFor(failure, TrainingField.PLANNED)) { edit { planned = it } }
            NumberField(R.string.tr_activity_actual, actual, ErrorFor(failure, TrainingField.ACTUAL)) { edit { actual = it } }
            NotesField(notes) { edit { notes = it } }
            ChoiceField(
                stringResource(R.string.tr_status),
                listOf(
                    ActivityStatus.PLANNED to stringResource(R.string.tr_status_planned),
                    ActivityStatus.COMPLETED to stringResource(R.string.tr_status_completed),
                    ActivityStatus.SKIPPED to stringResource(R.string.tr_status_skipped),
                ),
                status,
                onSelect = { edit { status = it } },
            )
        }
        failure.elsewhere(TrainingField.NAME, TrainingField.PLANNED, TrainingField.ACTUAL)?.let {
            Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
        }
        Button(
            onClick = {
                busy = true
                scope.launch {
                    failure = try {
                        app.core {
                            it.saveActivity(
                                sessionId,
                                activityId,
                                draft.copy(name = name, description = description, planned = planned, actual = actual, notes = notes, status = status),
                            )
                        }
                        onDone()
                        null
                    } catch (e: MobileException) {
                        e.failure()
                    }
                    busy = false
                }
            },
            enabled = !busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
        ) { Text(stringResource(R.string.save)) }
    }
}

/** The search of the activities written before: a touch on one fills the
 * form with a copy. */
@Composable
private fun CopyActivity(teamId: String, onCopy: (ActivityDraft) -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    var query by rememberSaveable { mutableStateOf("") }
    var onlyTeam by remember { mutableStateOf(false) }
    // the setting is read once, and written when it changes
    val scope = rememberCoroutineScope()
    LaunchedEffect(Unit) {
        onlyTeam = try {
            app.core { it.activitySearchTeamOnly() }
        } catch (_: MobileException) {
            false
        }
    }
    val hits by produceState(emptyList<ActivityHitRecord>(), query, onlyTeam) {
        value = if (query.isBlank()) emptyList() else try {
            app.core { it.searchActivities(teamId, query) }
        } catch (_: MobileException) {
            emptyList()
        }
    }
    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
        OutlinedTextField(
            query,
            { query = it },
            Modifier.fillMaxWidth(),
            label = { Text(stringResource(R.string.tr_copy_activity)) },
            placeholder = { Text(stringResource(R.string.tr_search)) },
            singleLine = true,
        )
        CheckRow(stringResource(R.string.tr_only_this_team), onlyTeam, onChecked = { only ->
            onlyTeam = only
            scope.launch {
                try {
                    app.core { it.setActivitySearchTeamOnly(only) }
                } catch (_: MobileException) {
                }
            }
        })
        hits.take(6).forEach { hit ->
            ListItem(
                modifier = Modifier.clickable {
                    scope.launch {
                        try {
                            onCopy(app.core { it.activityDraftCopying(hit) })
                        } catch (_: MobileException) {
                        }
                    }
                },
                headlineContent = { Text(hit.name) },
                supportingContent = { Text("${hit.teamName} · ${momentText(hit.sessionStart)}") },
            )
        }
    }
}

/** The new series of sessions: a band for each weekday, between two days. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SeriesScreen(teamId: String, onBack: () -> Unit, onDone: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    val places by produceState(emptyList<String>(), teamId) {
        value = try {
            app.core { it.trainingLocations(teamId) }
        } catch (_: MobileException) {
            emptyList()
        }
    }
    var from by rememberSaveable { mutableStateOf<LocalDate?>(LocalDate.now()) }
    var to by rememberSaveable { mutableStateOf<LocalDate?>(LocalDate.now().plusMonths(1)) }
    var location by rememberSaveable { mutableStateOf("") }
    // the time and the minutes of each weekday: no time, no session
    var times by rememberSaveable { mutableStateOf(List<LocalTime?>(7) { null }) }
    var minutes by rememberSaveable { mutableStateOf(List(7) { "90" }) }
    var failure by remember { mutableStateOf<Failure?>(null) }
    var conflicts by remember { mutableStateOf<List<SeriesConflictRecord>?>(null) }
    var busy by remember { mutableStateOf(false) }
    fun create(others: Boolean) {
        val bands = DayOfWeek.entries.mapNotNull { d ->
            val i = d.ordinal
            times[i]?.let { t ->
                val m = minutes[i].trim().toUShortOrNull()
                if (m == null) {
                    failure = Failure(TrainingField.BANDS, minutes[i])
                    return
                }
                SeriesBand((i + 1).toUByte(), t.format(HOUR_MINUTE), m)
            }
        }
        val start = from
        val end = to
        if (start == null || end == null) return
        busy = true
        scope.launch {
            failure = try {
                val result = app.core {
                    it.createSeries(teamId, SeriesDraft(start.toString(), end.toString(), location, bands), others)
                }
                when {
                    // nothing written: the user decides
                    result.conflicts.isNotEmpty() && result.created.isEmpty() && !others -> conflicts = result.conflicts
                    else -> onDone()
                }
                null
            } catch (e: MobileException) {
                e.failure()
            }
            busy = false
        }
    }
    val changed = location.isNotEmpty() || times.any { it != null }
    val back = rememberFormBack(changed, onBack)
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.tr_repeat_sessions), Modifier.title()) },
                navigationIcon = { BackButton(back) },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).imePadding()) {
            Column(
                Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                DayField(stringResource(R.string.tr_from), from, onDay = { d -> d?.let { from = it } }, error = ErrorFor(failure, TrainingField.RANGE))
                DayField(stringResource(R.string.tr_to), to, onDay = { d -> d?.let { to = it } })
                SuggestField(location, stringResource(R.string.tr_session_location), places, null, onChange = { location = it }, onPick = { location = it })
                Text(stringResource(R.string.tr_band_hint), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                DayOfWeek.entries.forEach { d ->
                    val i = d.ordinal
                    Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                        Text(d.getDisplayName(TextStyle.FULL, Locale.getDefault()), style = MaterialTheme.typography.titleSmall)
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            TimeField(
                                stringResource(R.string.tr_time),
                                times[i],
                                onTime = { t -> times = times.toMutableList().also { it[i] = t } },
                                modifier = Modifier.weight(1f),
                                clear = stringResource(R.string.cancel),
                            )
                            OutlinedTextField(
                                minutes[i],
                                { m -> minutes = minutes.toMutableList().also { it[i] = m } },
                                Modifier.weight(1f),
                                label = { Text(stringResource(R.string.tr_minutes)) },
                                singleLine = true,
                                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                            )
                        }
                    }
                }
            }
            failure.elsewhere(TrainingField.RANGE)?.let {
                Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
            }
            Button(
                onClick = { create(false) },
                enabled = !busy,
                modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
            ) { Text(stringResource(R.string.save)) }
        }
        conflicts?.let { list ->
            AlertDialog(
                onDismissRequest = { conflicts = null },
                title = { Text(stringResource(R.string.tr_series_conflicts)) },
                text = {
                    Column(Modifier.verticalScroll(rememberScrollState())) {
                        list.forEach { Text(momentText(it.start)) }
                    }
                },
                confirmButton = {
                    TextButton(onClick = {
                        conflicts = null
                        create(true)
                    }) { Text(stringResource(R.string.tr_create_others)) }
                },
                dismissButton = { TextButton(onClick = { conflicts = null }) { Text(stringResource(R.string.cancel)) } },
            )
        }
    }
}

/** Where a session is copied: a day and time, the microcycle, what is
 * copied. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DuplicateScreen(teamId: String, sessionId: String, onBack: () -> Unit, onDuplicated: (String) -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val proposed by produceState<io.github.naighes.scout4all.core.DuplicateDraft?>(null, sessionId) {
        value = try {
            app.core { it.proposedDuplicate(sessionId) }
        } catch (_: MobileException) {
            null
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.tr_duplicate_session), Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        val p = proposed
        if (p == null) Centered(modifier) { CircularProgressIndicator() }
        else DuplicateFields(teamId, sessionId, p, modifier, onDuplicated)
    }
}

@Composable
private fun DuplicateFields(
    teamId: String,
    sessionId: String,
    proposed: io.github.naighes.scout4all.core.DuplicateDraft,
    modifier: Modifier,
    onDuplicated: (String) -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    val start = remember { LocalDateTime.parse(proposed.start) }
    var day by rememberSaveable { mutableStateOf<LocalDate?>(start.toLocalDate()) }
    var time by rememberSaveable { mutableStateOf<LocalTime?>(start.toLocalTime()) }
    var micro by rememberSaveable { mutableStateOf(proposed.microcycleId) }
    var objectives by rememberSaveable { mutableStateOf(proposed.copyObjectives) }
    var activities by rememberSaveable { mutableStateOf(proposed.copyActivities) }
    var failure by remember { mutableStateOf<Failure?>(null) }
    var busy by remember { mutableStateOf(false) }
    val options = rememberMicrocycles(teamId, day ?: start.toLocalDate(), micro)
    Column(modifier) {
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            DayField(stringResource(R.string.date), day, onDay = { d -> day = d }, error = ErrorFor(failure, TrainingField.START))
            TimeField(stringResource(R.string.tr_time), time, onTime = { time = it })
            MicrocycleChoice(options, micro) { micro = it }
            CheckRow(stringResource(R.string.tr_copy_objectives), objectives, onChecked = { objectives = it })
            CheckRow(stringResource(R.string.tr_copy_activities), activities, onChecked = { activities = it })
        }
        failure.elsewhere(TrainingField.START)?.let {
            Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
        }
        Button(
            onClick = {
                val d = day
                val t = time
                if (d == null || t == null) return@Button
                busy = true
                scope.launch {
                    failure = try {
                        val copy = app.core {
                            it.duplicateSession(
                                sessionId,
                                proposed.copy(
                                    start = "${d}T${t.format(HOUR_MINUTE)}",
                                    microcycleId = micro,
                                    copyObjectives = objectives,
                                    copyActivities = activities,
                                ),
                            )
                        }
                        onDuplicated(copy.id)
                        null
                    } catch (e: MobileException) {
                        e.failure()
                    }
                    busy = false
                }
            },
            enabled = !busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
        ) { Text(stringResource(R.string.tr_duplicate_session)) }
    }
}

/** What changes in the series of a session: for which sessions, what, or
 * to remove the planned ones instead. `onDone` tells whether the session
 * is gone. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SeriesChangeScreen(teamId: String, sessionId: String, onBack: () -> Unit, onDone: (gone: Boolean) -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val session by produceState<io.github.naighes.scout4all.core.SessionRecord?>(null, sessionId) {
        value = try {
            app.core { it.trainingSession(sessionId) }
        } catch (_: MobileException) {
            null
        }
    }
    val places by produceState(emptyList<String>(), teamId) {
        value = try {
            app.core { it.trainingLocations(teamId) }
        } catch (_: MobileException) {
            emptyList()
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.tr_edit_series), Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        val s = session
        if (s == null) Centered(modifier) { CircularProgressIndicator() }
        else SeriesChangeFields(s, places, modifier, onDone)
    }
}

@Composable
private fun SeriesChangeFields(
    s: io.github.naighes.scout4all.core.SessionRecord,
    places: List<String>,
    modifier: Modifier,
    onDone: (gone: Boolean) -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    val own = remember { LocalDateTime.parse(s.start) }
    var range by rememberSaveable { mutableStateOf(io.github.naighes.scout4all.core.SeriesScope.THIS) }
    var location by rememberSaveable { mutableStateOf(s.location) }
    var time by rememberSaveable { mutableStateOf<LocalTime?>(own.toLocalTime()) }
    var minutes by rememberSaveable { mutableStateOf(s.durationMin.toString()) }
    var remove by rememberSaveable { mutableStateOf(false) }
    var failure by remember { mutableStateOf<Failure?>(null) }
    var busy by remember { mutableStateOf(false) }
    Column(modifier) {
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            ChoiceField(
                stringResource(R.string.tr_series_scope),
                listOf(
                    io.github.naighes.scout4all.core.SeriesScope.THIS to stringResource(R.string.tr_scope_this),
                    io.github.naighes.scout4all.core.SeriesScope.THIS_AND_FOLLOWING to stringResource(R.string.tr_scope_following),
                    io.github.naighes.scout4all.core.SeriesScope.ALL to stringResource(R.string.tr_scope_all),
                ),
                range,
                onSelect = { range = it },
            )
            if (!remove) {
                SuggestField(location, stringResource(R.string.tr_session_location), places, null, onChange = { location = it }, onPick = { location = it })
                TimeField(stringResource(R.string.tr_time), time, onTime = { t -> t?.let { time = it } })
                NumberField(R.string.tr_session_duration, minutes, ErrorFor(failure, TrainingField.DURATION)) { minutes = it }
            }
            CheckRow(stringResource(R.string.tr_remove_series_check), remove, onChecked = { remove = it })
        }
        failure.elsewhere(TrainingField.DURATION)?.let {
            Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
        }
        Button(
            onClick = {
                busy = true
                scope.launch {
                    failure = try {
                        // only what is different is sent
                        val change = io.github.naighes.scout4all.core.SeriesChange(
                            location = location.takeIf { it.trim() != s.location },
                            time = time?.takeIf { it != own.toLocalTime() }?.format(HOUR_MINUTE),
                            durationMin = minutes.trim().toUShortOrNull()?.takeIf { it != s.durationMin },
                        )
                        val gone = app.core {
                            if (remove) {
                                it.removeSeries(s.id, range)
                                true
                            } else {
                                it.updateSeries(s.id, range, change)
                                false
                            }
                        }
                        onDone(gone)
                        null
                    } catch (e: MobileException) {
                        e.failure()
                    }
                    busy = false
                }
            },
            enabled = !busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
        ) { Text(stringResource(if (remove) R.string.delete_forever else R.string.save)) }
    }
}
