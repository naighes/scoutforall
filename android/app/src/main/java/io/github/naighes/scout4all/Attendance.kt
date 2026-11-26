package io.github.naighes.scout4all

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.filled.Search
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.BasicAlertDialog
import androidx.compose.material3.AlertDialogDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
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
import androidx.compose.runtime.mutableStateListOf
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
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.AttendanceRowRecord
import io.github.naighes.scout4all.core.AttendanceStatus
import io.github.naighes.scout4all.core.GuestChoice
import io.github.naighes.scout4all.core.MobileException
import kotlinx.coroutines.launch

@Composable
private fun AttendanceStatus.label() = stringResource(
    when (this) {
        AttendanceStatus.PRESENT -> R.string.tr_present
        AttendanceStatus.LATE -> R.string.tr_late
        AttendanceStatus.ABSENT -> R.string.tr_absent
        AttendanceStatus.UNAVAILABLE -> R.string.tr_unavailable
    },
)

/** The next state a touch gives: present, late, absent, unavailable. */
private fun AttendanceStatus.next() = when (this) {
    AttendanceStatus.PRESENT -> AttendanceStatus.LATE
    AttendanceStatus.LATE -> AttendanceStatus.ABSENT
    AttendanceStatus.ABSENT -> AttendanceStatus.UNAVAILABLE
    AttendanceStatus.UNAVAILABLE -> AttendanceStatus.PRESENT
}

/** The attendance of a session: a row for each player, big, a touch on
 * the state changes it (the gym, one hand); the note, the guests and the
 * players who joined later are in the same screen. Nothing is written until
 * saved. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AttendanceScreen(teamId: String, sessionId: String, onBack: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    val sheet by produceState<io.github.naighes.scout4all.core.AttendanceSheetRecord?>(null, sessionId) {
        value = try {
            app.core { it.sessionAttendance(sessionId) }
        } catch (_: MobileException) {
            null
        }
    }
    val rows = remember { mutableStateListOf<AttendanceRowRecord>() }
    var loaded by remember { mutableStateOf(false) }
    if (!loaded) sheet?.let {
        rows.addAll(it.rows)
        loaded = true
    }
    var changed by rememberSaveable { mutableStateOf(false) }
    val back = rememberFormBack(changed, onBack)
    var noting by remember { mutableStateOf<Int?>(null) }
    var guest by rememberSaveable { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val missing by produceState(emptyList<AttendanceRowRecord>(), sessionId, rows.size) {
        value = try {
            // the saved ones only: the roster of a list not saved is whole
            if (sheet?.recorded == true) app.core { it.missingPlayers(sessionId) } else emptyList()
        } catch (_: MobileException) {
            emptyList()
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.tr_attendance), Modifier.title()) },
                navigationIcon = { BackButton(back) },
                actions = {
                    IconButton(onClick = { guest = true }) {
                        Icon(Icons.Filled.Add, contentDescription = stringResource(R.string.tr_add_guest))
                    }
                },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        if (!loaded) {
            Centered(modifier) { CircularProgressIndicator() }
            return@Scaffold
        }
        Column(modifier) {
            if (sheet?.recorded == false) {
                Text(
                    stringResource(R.string.tr_attendance_not_recorded),
                    Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            val absent = missing.filter { m -> rows.none { it.personId == m.personId } }
            if (absent.isNotEmpty()) {
                FilledTonalButton(
                    onClick = {
                        rows.addAll(absent)
                        changed = true
                    },
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp),
                ) { Text("${stringResource(R.string.tr_add_missing)} (${absent.size})") }
            }
            LazyColumn(Modifier.weight(1f)) {
                items(rows.size, key = { rows[it].personId }) { i ->
                    val row = rows[i]
                    PlayerRow(
                        row,
                        onCycle = {
                            rows[i] = row.copy(status = row.status.next())
                            changed = true
                        },
                        onNote = { noting = i },
                    )
                }
            }
            error?.let { Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error) }
            Button(
                onClick = {
                    scope.launch {
                        try {
                            app.core { it.saveAttendance(sessionId, rows.toList()) }
                            onBack()
                        } catch (e: MobileException) {
                            error = e.reason
                        }
                    }
                },
                modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
            ) { Text(stringResource(R.string.save)) }
        }
        noting?.takeIf { it in rows.indices }?.let { i ->
            NoteDialog(
                rows[i],
                onDismiss = { noting = null },
                onSave = { note, makeup ->
                    rows[i] = rows[i].copy(note = note, makeup = makeup)
                    changed = true
                    noting = null
                },
                // a guest leaves the session
                onRemove = rows[i].guest.takeIf { it }?.let {
                    {
                        rows.removeAt(i)
                        changed = true
                        noting = null
                    }
                },
            )
        }
        if (guest) {
            GuestDialog(
                teamId,
                taken = rows.map { it.personId }.toSet(),
                onDismiss = { guest = false },
                onPick = { choice ->
                    rows.add(
                        AttendanceRowRecord(
                            choice.personId,
                            choice.name,
                            choice.teams.firstOrNull()?.number,
                            AttendanceStatus.PRESENT,
                            "",
                            true,
                            // a player of another team of the user makes up by default
                            choice.teams.isNotEmpty(),
                        ),
                    )
                    changed = true
                    guest = false
                },
            )
        }
    }
}

/** A player: the number, the name, the note, and the state to touch. */
@Composable
private fun PlayerRow(row: AttendanceRowRecord, onCycle: () -> Unit, onNote: () -> Unit) {
    val present = row.status == AttendanceStatus.PRESENT || row.status == AttendanceStatus.LATE
    ListItem(
        modifier = Modifier.heightIn(min = 64.dp),
        leadingContent = {
            Box(Modifier.width(40.dp), contentAlignment = Alignment.CenterEnd) {
                Text(row.number?.toString().orEmpty(), style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
            }
        },
        headlineContent = { Text(row.name) },
        supportingContent = {
            val line = listOfNotNull(
                stringResource(R.string.tr_guest).takeIf { row.guest },
                stringResource(R.string.tr_makeup).takeIf { row.guest && row.makeup },
                row.note.takeIf { it.isNotBlank() },
            ).joinToString(" · ")
            if (line.isNotEmpty()) Text(line)
        },
        trailingContent = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                IconButton(onClick = onNote) {
                    Icon(Icons.Filled.Edit, contentDescription = stringResource(R.string.tr_attendance_note))
                }
                // one width for the short words, so the pencils stay in a column
                // and a narrower padding: the name keeps its line
                val wide = Modifier.widthIn(min = 96.dp)
                val inside = PaddingValues(horizontal = 14.dp)
                if (present) {
                    Button(onClick = onCycle, modifier = wide, contentPadding = inside) { Text(row.status.label()) }
                } else {
                    OutlinedButton(onClick = onCycle, modifier = wide, contentPadding = inside) { Text(row.status.label()) }
                }
            }
        },
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun NoteDialog(
    row: AttendanceRowRecord,
    onDismiss: () -> Unit,
    onSave: (String, Boolean) -> Unit,
    onRemove: (() -> Unit)?,
) {
    var note by rememberSaveable { mutableStateOf(row.note) }
    // a guest can make up for a session of their own team
    var makeup by rememberSaveable { mutableStateOf(row.makeup) }
    // not the width of the platform: a field and a box together in such a dialog never get idle under
    // Robolectric on SDK 28 (an AlertDialog can't drop it, its own margins kept by hand)
    BasicAlertDialog(
        onDismissRequest = onDismiss,
        modifier = Modifier.padding(horizontal = 24.dp),
        properties = DialogProperties(usePlatformDefaultWidth = false),
    ) {
        Surface(
            shape = AlertDialogDefaults.shape,
            color = AlertDialogDefaults.containerColor,
            tonalElevation = AlertDialogDefaults.TonalElevation,
        ) {
            Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(
                    row.name,
                    Modifier.padding(bottom = 8.dp),
                    style = MaterialTheme.typography.headlineSmall,
                    color = AlertDialogDefaults.titleContentColor,
                )
                OutlinedTextField(
                    note,
                    { note = it },
                    Modifier.fillMaxWidth(),
                    label = { Text(stringResource(R.string.tr_attendance_note)) },
                    minLines = 2,
                )
                if (row.guest) {
                    CheckRow(stringResource(R.string.tr_makeup), makeup, { makeup = it })
                }
                Row(Modifier.fillMaxWidth().padding(top = 8.dp), horizontalArrangement = Arrangement.End) {
                    onRemove?.let { TextButton(onClick = it) { Text(stringResource(R.string.tr_remove_guest)) } }
                    TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
                    TextButton(onClick = { onSave(note.trim(), makeup) }) { Text(stringResource(R.string.ok)) }
                }
            }
        }
    }
}

/** The players of other teams (the registry) to ask as guests, or a new person in no team with the name written:
 * on the whole screen like the countries, the list long and the keyboard up. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun GuestDialog(teamId: String, taken: Set<String>, onDismiss: () -> Unit, onPick: (GuestChoice) -> Unit) =
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        val app = LocalContext.current.applicationContext as Scout4AllApplication
        val choices by produceState(emptyList<GuestChoice>(), teamId) {
            value = try {
                app.core { it.guestChoices(teamId) }
            } catch (_: MobileException) {
                emptyList()
            }
        }
        val scope = rememberCoroutineScope()
        var error by remember { mutableStateOf<String?>(null) }
        var query by rememberSaveable { mutableStateOf("") }
        val written = query.trim()
        // as many as the desktop offers, the new person after them
        val shown = choices
            .filter { it.personId !in taken && it.name.contains(written, ignoreCase = true) }
            .take(GUESTS_OFFERED)
        val focus = remember { FocusRequester() }
        LaunchedEffect(Unit) { focus.requestFocus() }
        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Text(stringResource(R.string.tr_add_guest), Modifier.title()) },
                    navigationIcon = { BackButton(onDismiss) },
                )
            },
        ) { padding ->
            Column(Modifier.fillMaxSize().padding(padding).imePadding()) {
                OutlinedTextField(
                    value = query,
                    onValueChange = {
                        query = it
                        error = null
                    },
                    modifier = Modifier.fillMaxWidth().padding(16.dp).focusRequester(focus),
                    placeholder = { Text(stringResource(R.string.tr_search)) },
                    leadingIcon = { Icon(Icons.Filled.Search, contentDescription = null) },
                    singleLine = true,
                    isError = error != null,
                    supportingText = noteOf(error),
                    keyboardOptions = KeyboardOptions(
                        capitalization = KeyboardCapitalization.Words,
                        imeAction = ImeAction.Search,
                    ),
                )
                LazyColumn(Modifier.weight(1f)) {
                    items(shown, key = { it.personId }) { c ->
                        ListItem(
                            modifier = Modifier.clickable { onPick(c) },
                            headlineContent = { Text(c.name) },
                            supportingContent = {
                                Text(c.teams.joinToString(" · ") { "${it.name} ${it.year} #${it.number}" })
                            },
                        )
                    }
                    if (written.isNotEmpty()) {
                        item(key = "new") {
                            ListItem(
                                modifier = Modifier.clickable {
                                    scope.launch {
                                        try {
                                            onPick(app.core { it.createGuest(written) })
                                        } catch (e: MobileException) {
                                            error = e.reason
                                        }
                                    }
                                },
                                leadingContent = { Icon(Icons.Filled.Add, contentDescription = null) },
                                headlineContent = { Text(stringResource(R.string.tr_new_person, written)) },
                            )
                        }
                    }
                }
            }
        }
    }

/** The guests offered at once, as on the desktop: what's typed narrows them. */
private const val GUESTS_OFFERED = 20
