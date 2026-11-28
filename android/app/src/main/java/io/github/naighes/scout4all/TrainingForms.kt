package io.github.naighes.scout4all

import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.RadioButton
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.semantics.Role
import io.github.naighes.scout4all.core.SeriesScope
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
import io.github.naighes.scout4all.core.VenueClashRecord
import kotlinx.coroutines.launch
import java.time.DayOfWeek
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.LocalTime
import java.time.format.TextStyle
import java.util.Locale

/** What an entry of a series may miss, each on its field. */
private enum class EntryField { DAY, TIME, DURATION, UNTIL }

/** The text under the field an error is about, or at the bottom. */
internal class Failure(val field: TrainingField?, val text: String)

internal fun MobileException.failure() = Failure((this as? MobileException.InvalidTraining)?.field, reason)

internal fun ErrorFor(failure: Failure?, vararg fields: TrainingField): String? =
    failure?.takeIf { it.field in fields }?.text

internal fun Failure?.elsewhere(vararg fields: TrainingField): String? =
    this?.takeIf { it.field == null || it.field !in fields }?.text

@Composable
internal fun TrainingSection(title: Int) = Text(
    stringResource(title),
    Modifier.title(),
    style = MaterialTheme.typography.titleMedium,
    color = MaterialTheme.colorScheme.primary,
)

/** The form of a session, new (no `sessionId`) or edited. A new one may be
 * a weekly series instead: an entry after the other (Continue), all made
 * at the end (Finish, then `onRepeated`), each entry a series of its own.
 * A saved one in a series asks where its time, duration and gym go. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SessionFormScreen(
    teamId: String,
    sessionId: String?,
    onBack: () -> Unit,
    onSaved: (String) -> Unit,
    day: LocalDate? = null,
    time: LocalTime? = null,
    onRepeated: () -> Unit = onBack,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val draft by produceState<SessionDraft?>(null, sessionId) {
        value = try {
            app.core { if (sessionId == null) it.newSessionDraft() else it.sessionDraft(sessionId) }
        } catch (_: MobileException) {
            null
        }
    }
    var changed by rememberSaveable { mutableStateOf(false) }
    var series by rememberSaveable { mutableStateOf(false) }
    // the entries of the series written before the one in the form
    val entries = remember { mutableStateListOf<SeriesDraft>() }
    val back = rememberFormBack(changed || entries.isNotEmpty(), onBack)
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text(
                        when {
                            sessionId != null -> stringResource(R.string.tr_edit_session)
                            series -> stringResource(R.string.tr_new_series) + " · " +
                                stringResource(R.string.tr_series_entry, entries.size + 1)
                            else -> stringResource(R.string.tr_new_session)
                        },
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
        else SessionFields(
            teamId, sessionId, d, modifier, day, time,
            series = series && sessionId == null,
            onSeries = { series = it },
            entries = entries,
            onChanged = { changed = true },
            onSaved = onSaved,
            onRepeated = onRepeated,
        )
    }
}

@Composable
private fun SessionFields(
    teamId: String,
    sessionId: String?,
    draft: SessionDraft,
    modifier: Modifier,
    initialDay: LocalDate?,
    initialTime: LocalTime?,
    series: Boolean,
    onSeries: (Boolean) -> Unit,
    entries: MutableList<SeriesDraft>,
    onChanged: () -> Unit,
    onSaved: (String) -> Unit,
    onRepeated: () -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    // a new session: in the next hour; the form of the core has no day
    val first = remember {
        draft.start?.let(LocalDateTime::parse)
            ?: initialDay?.atTime(initialTime ?: LocalTime.of(18, 0))
            ?: app.clock.now().plusHours(1).withMinute(0).withSecond(0).withNano(0)
    }
    val busyDays = rememberBusyDays(teamId)
    var day by rememberSaveable { mutableStateOf<LocalDate?>(first.toLocalDate()) }
    var time by rememberSaveable { mutableStateOf<LocalTime?>(first.toLocalTime()) }
    var until by rememberSaveable { mutableStateOf<LocalDate?>(null) }
    var duration by rememberSaveable { mutableStateOf(draft.duration) }
    var actual by rememberSaveable { mutableStateOf(draft.actualDuration) }
    var venue by rememberSaveable { mutableStateOf(draft.venue) }
    var clashes by remember { mutableStateOf<List<VenueClashRecord>?>(null) }
    var title by rememberSaveable { mutableStateOf(draft.title) }
    var objectives by rememberSaveable { mutableStateOf(draft.objectives) }
    var notes by rememberSaveable { mutableStateOf(draft.notes) }
    var remarks by rememberSaveable { mutableStateOf(draft.finalRemarks) }
    var micro by rememberSaveable { mutableStateOf(draft.microcycle) }
    var touched by rememberSaveable { mutableStateOf(false) }
    var failure by remember { mutableStateOf<Failure?>(null) }
    // the field to correct for the entry of a series to be whole
    var wrong by remember { mutableStateOf<EntryField?>(null) }
    var conflicts by remember { mutableStateOf<List<SeriesConflictRecord>?>(null) }
    // the draft of a session of a series, waiting for where it goes
    var asking by remember { mutableStateOf<SessionDraft?>(null) }
    var busy by remember { mutableStateOf(false) }
    val options = rememberMicrocycles(teamId, day ?: first.toLocalDate(), micro)
    // a new session takes the microcycle of its day, until chosen otherwise
    LaunchedEffect(options) {
        if (sessionId == null && !touched) micro = options.firstOrNull()?.id
    }
    fun edit(change: () -> Unit) {
        change()
        wrong = null
        onChanged()
    }
    fun current(): SessionDraft = draft.copy(
        start = day?.let { d -> time?.let { t -> "${d}T${t.format(HOUR_MINUTE)}" } },
        duration = duration,
        actualDuration = actual,
        venue = venue,
        title = title,
        objectives = objectives,
        notes = notes,
        finalRemarks = remarks,
        microcycle = micro,
    )
    // the session written; its time, duration and gym to the others of its
    // series the scope reaches
    fun write(written: SessionDraft, range: SeriesScope) {
        busy = true
        scope.launch {
            failure = try {
                val saved = app.core {
                    when {
                        sessionId != null && range != SeriesScope.THIS -> it.saveSessionInSeries(sessionId, written, range)
                        else -> it.saveSession(teamId, sessionId, written)
                    }
                }
                onSaved(saved.id)
                null
            } catch (e: MobileException) {
                e.failure()
            }
            busy = false
        }
    }
    // a gym held by another team asks first, once
    fun save(confirmed: Boolean) {
        busy = true
        scope.launch {
            failure = try {
                val written = current()
                val start = written.start
                val busyWith = when {
                    confirmed || venue == null || start == null -> emptyList()
                    else -> duration.trim().toUShortOrNull()?.let { m ->
                        app.core { it.sessionVenueClashes(teamId, venue!!, start, m) }
                    }.orEmpty()
                }
                val asks = busyWith.isEmpty() && sessionId != null &&
                    app.core { it.asksSeriesScope(sessionId, written) }
                when {
                    busyWith.isNotEmpty() -> clashes = busyWith
                    asks -> asking = written
                    else -> {
                        val saved = app.core { it.saveSession(teamId, sessionId, written) }
                        onSaved(saved.id)
                    }
                }
                null
            } catch (e: MobileException) {
                e.failure()
            }
            busy = false
        }
    }
    // the entry in the form, if whole: else its first field to correct
    fun entry(): SeriesDraft? {
        fun refused(field: EntryField): Nothing? {
            wrong = field
            return null
        }
        val d = day ?: return refused(EntryField.DAY)
        val t = time ?: return refused(EntryField.TIME)
        val m = duration.trim().toUShortOrNull()?.takeIf { it > 0u } ?: return refused(EntryField.DURATION)
        val last = until?.takeIf { !it.isBefore(d) } ?: return refused(EntryField.UNTIL)
        return SeriesDraft(d.toString(), last.toString(), listOf(SeriesBand(d.dayOfWeek.value.toUByte(), t.format(HOUR_MINUTE), m, venue)))
    }
    // Continue: the entry kept, the form ready for the next (the day after)
    fun next() {
        val e = entry() ?: return
        entries.add(e)
        day = day?.plusDays(1)
        wrong = null
        onChanged()
    }
    fun make(all: List<SeriesDraft>, others: Boolean, confirmed: Boolean) {
        busy = true
        scope.launch {
            failure = try {
                val gyms = all.any { e -> e.bands.any { it.venue != null } }
                val busyWith = if (confirmed || !gyms) emptyList() else app.core { it.seriesVenueClashes(teamId, all) }
                val result = when {
                    busyWith.isNotEmpty() -> null
                    else -> app.core { it.createSeries(teamId, all, others) }
                }
                when {
                    result == null -> clashes = busyWith
                    // nothing written: the user decides
                    result.created.isEmpty() && result.conflicts.isNotEmpty() -> conflicts = result.conflicts
                    else -> onRepeated()
                }
                null
            } catch (e: MobileException) {
                e.failure()
            }
            busy = false
        }
    }
    // Finish: all the entries, the one in the form last
    fun finish(others: Boolean = false, confirmed: Boolean = false) {
        val e = entry() ?: return
        make(entries + e, others, confirmed)
    }
    val entryNote = stringResource(R.string.tr_entry_invalid)
    fun noteFor(field: EntryField): String? = entryNote.takeIf { series && wrong == field }
    clashes?.let {
        ClashDialog(it, onConfirm = {
            clashes = null
            if (series) finish(confirmed = true) else save(true)
        }, onDismiss = { clashes = null })
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
                    finish(others = true, confirmed = true)
                }) { Text(stringResource(R.string.tr_create_others)) }
            },
            dismissButton = { TextButton(onClick = { conflicts = null }) { Text(stringResource(R.string.cancel)) } },
        )
    }
    asking?.let { written ->
        ScopeDialog(
            stringResource(R.string.tr_scope_edit_text),
            stringResource(R.string.save),
            deleting = false,
            onChosen = { range ->
                asking = null
                write(written, range)
            },
            onDismiss = { asking = null },
        )
    }
    Column(modifier) {
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            if (sessionId == null) {
                val kinds = listOf(R.string.tr_single_session, R.string.tr_weekly_series)
                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                    kinds.forEachIndexed { index, kind ->
                        SegmentedButton(
                            selected = series == (index == 1),
                            onClick = { onSeries(index == 1) },
                            shape = SegmentedButtonDefaults.itemShape(index, kinds.size),
                        ) { Text(stringResource(kind)) }
                    }
                }
            }
            if (series && entries.isNotEmpty()) SeriesEntries(entries)
            DayField(
                stringResource(R.string.date),
                day,
                onDay = { d -> edit { day = d } },
                error = noteFor(EntryField.DAY) ?: ErrorFor(failure, TrainingField.START),
                busy = busyDays,
            )
            TimeField(stringResource(R.string.tr_time), time, onTime = { t -> edit { time = t } }, error = noteFor(EntryField.TIME))
            NumberField(R.string.tr_session_duration, duration, noteFor(EntryField.DURATION) ?: ErrorFor(failure, TrainingField.DURATION)) { edit { duration = it } }
            VenueChoice(teamId, venue, onVenue = { v -> edit { venue = v } })
            if (series) {
                DayField(stringResource(R.string.tr_until), until, onDay = { d -> edit { until = d } }, error = noteFor(EntryField.UNTIL), busy = busyDays)
            } else {
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
        }
        FailureDialog(failure.elsewhere(TrainingField.START, TrainingField.DURATION, TrainingField.ACTUAL_DURATION)) { failure = null }
        // big and at the bottom, where the thumb is
        if (series) {
            Row(Modifier.fillMaxWidth().padding(16.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedButton(
                    onClick = { next() },
                    enabled = !busy,
                    modifier = Modifier.weight(1f).heightIn(min = 56.dp),
                ) { Text(stringResource(R.string.tr_continue_entry)) }
                Button(
                    onClick = { finish() },
                    enabled = !busy,
                    modifier = Modifier.weight(1f).heightIn(min = 56.dp),
                ) { Text(stringResource(R.string.tr_finish)) }
            }
        } else {
            Button(
                onClick = { save(false) },
                enabled = !busy,
                modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
            ) { Text(stringResource(R.string.save)) }
        }
    }
}

/** The entries of a series written before: weekday and time, minutes, the
 * days. */
@Composable
private fun SeriesEntries(entries: List<SeriesDraft>) = Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
    Text(stringResource(R.string.tr_series_entries), style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary)
    entries.forEach { e ->
        e.bands.forEach { b ->
            val weekday = DayOfWeek.of(b.weekday.toInt().coerceIn(1, 7)).getDisplayName(TextStyle.FULL, Locale.getDefault())
            Text(
                "$weekday ${b.time} · ${b.durationMin}′ · ${dayText(LocalDate.parse(e.from))} – ${dayText(LocalDate.parse(e.to))}",
                style = MaterialTheme.typography.bodyMedium,
            )
        }
    }
}

/** The question over a session of a series, on changing or deleting it:
 * only this session, this and the following, the whole series. */
@Composable
internal fun ScopeDialog(
    text: String,
    confirm: String,
    deleting: Boolean,
    onChosen: (SeriesScope) -> Unit,
    onDismiss: () -> Unit,
) {
    var chosen by rememberSaveable { mutableStateOf(SeriesScope.THIS) }
    val scopes = listOf(
        SeriesScope.THIS to R.string.tr_scope_this,
        SeriesScope.THIS_AND_FOLLOWING to R.string.tr_scope_following,
        SeriesScope.ALL to R.string.tr_scope_all,
    )
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.tr_in_series_title)) },
        text = {
            Column(Modifier.selectableGroup()) {
                Text(text)
                scopes.forEach { (range, label) ->
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = 48.dp)
                            .selectable(selected = chosen == range, role = Role.RadioButton, onClick = { chosen = range }),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        RadioButton(selected = chosen == range, onClick = null)
                        Text(stringResource(label), Modifier.padding(start = 12.dp))
                    }
                }
            }
        },
        confirmButton = {
            Button(
                onClick = { onChosen(chosen) },
                colors = when (deleting) {
                    true -> ButtonDefaults.buttonColors(
                        containerColor = MaterialTheme.colorScheme.error,
                        contentColor = MaterialTheme.colorScheme.onError,
                    )
                    false -> ButtonDefaults.buttonColors()
                },
            ) { Text(confirm) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

@Composable
internal fun NumberField(label: Int, value: String, error: String?, onChange: (String) -> Unit) = OutlinedTextField(
    value,
    onChange,
    Modifier.fillMaxWidth(),
    label = { Text(stringResource(label)) },
    singleLine = true,
    isError = error != null,
    supportingText = noteOf(error),
    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Next),
)

@Composable
internal fun Lines(label: Int, value: String, onChange: (String) -> Unit) = OutlinedTextField(
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
                            Icon(Icons.Filled.Delete, contentDescription = stringResource(R.string.delete))
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
                supportingText = noteOf(ErrorFor(failure, TrainingField.NAME)),
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
        FailureDialog(failure.elsewhere(TrainingField.NAME, TrainingField.PLANNED, TrainingField.ACTUAL)) { failure = null }
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
    var clashes by remember { mutableStateOf<List<VenueClashRecord>?>(null) }
    val options = rememberMicrocycles(teamId, day ?: start.toLocalDate(), micro)
    fun duplicate(confirmed: Boolean) {
        val d = day
        val t = time
        if (d == null || t == null) return
        busy = true
        scope.launch {
            failure = try {
                val draft = proposed.copy(
                    start = "${d}T${t.format(HOUR_MINUTE)}",
                    microcycleId = micro,
                    copyObjectives = objectives,
                    copyActivities = activities,
                )
                val busyWith = if (confirmed) emptyList() else app.core { it.duplicateVenueClashes(sessionId, draft) }
                if (busyWith.isNotEmpty()) {
                    clashes = busyWith
                } else {
                    onDuplicated(app.core { it.duplicateSession(sessionId, draft) }.id)
                }
                null
            } catch (e: MobileException) {
                e.failure()
            }
            busy = false
        }
    }
    clashes?.let { ClashDialog(it, onConfirm = { clashes = null; duplicate(true) }, onDismiss = { clashes = null }) }
    Column(modifier) {
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            DayField(stringResource(R.string.date), day, onDay = { d -> day = d }, error = ErrorFor(failure, TrainingField.START), busy = rememberBusyDays(teamId))
            TimeField(stringResource(R.string.tr_time), time, onTime = { time = it })
            MicrocycleChoice(options, micro) { micro = it }
            CheckRow(stringResource(R.string.tr_copy_objectives), objectives, onChecked = { objectives = it })
            CheckRow(stringResource(R.string.tr_copy_activities), activities, onChecked = { activities = it })
        }
        FailureDialog(failure.elsewhere(TrainingField.START)) { failure = null }
        Button(
            onClick = { duplicate(false) },
            enabled = !busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
        ) { Text(stringResource(R.string.tr_duplicate_session)) }
    }
}
