package io.github.naighes.scout4all

import android.content.ActivityNotFoundException
import android.content.Intent
import android.net.Uri
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowLeft
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Place
import androidx.compose.material.icons.filled.Star
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.foundation.layout.width
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
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
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.AgendaSpan
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.TrainingField
import io.github.naighes.scout4all.core.VenueClashRecord
import io.github.naighes.scout4all.core.VenueDay
import io.github.naighes.scout4all.core.VenueDraft
import io.github.naighes.scout4all.core.VenueRecord
import java.time.LocalDate
import java.time.LocalDateTime
import kotlinx.coroutines.launch

/** The name of a gym, "deleted gym" for a removed one; null while it
 * loads or without a gym. */
@Composable
internal fun venueLabel(id: String?): String? {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val gone = stringResource(R.string.venue_deleted)
    return produceState<String?>(null, id) {
        value = id?.let {
            try {
                app.core { core -> core.venue(it) }.let { v -> if (v.deleted) gone else v.name }
            } catch (_: MobileException) {
                null
            }
        }
    }.value
}

/** The map of the phone at the gym: the `geo:` address, else the page of
 * the link; nothing if neither opens. */
internal fun openVenueMap(app: android.content.Context, geo: String?, web: String?) {
    listOfNotNull(geo, web).forEach { address ->
        try {
            app.startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(address)).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
            return
        } catch (_: ActivityNotFoundException) {
        }
    }
}

/** "Open in the map" for the gym of a session or a match: the `geo:` address
 * (the map app of the phone), else the page of its link. */
@Composable
internal fun VenueMapButton(venueId: String) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    androidx.compose.material3.TextButton(onClick = {
        scope.launch {
            val (geo, web) = try {
                app.core { it.venueGeoUri(venueId) to it.venueWebUrl(venueId) }
            } catch (_: MobileException) {
                null to null
            }
            openVenueMap(app, geo, web)
        }
    }) {
        Icon(Icons.Filled.Place, contentDescription = null)
        androidx.compose.foundation.layout.Spacer(Modifier.width(8.dp))
        Text(stringResource(R.string.venue_open_map))
    }
}

/** The gym of a form, as the desktop offers it: the pinned first, then the
 * ones of the team, a search by name and a new gym with the name only. */
@Composable
fun VenueChoice(teamId: String, venue: String?, onVenue: (String?) -> Unit, modifier: Modifier = Modifier) {
    var open by rememberSaveable { mutableStateOf(false) }
    val name = venueLabel(venue).orEmpty()
    Row(modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        // a touch on the read-only field opens the choice
        val touches = remember { androidx.compose.foundation.interaction.MutableInteractionSource() }
        androidx.compose.runtime.LaunchedEffect(touches) {
            touches.interactions.collect {
                if (it is androidx.compose.foundation.interaction.PressInteraction.Release) open = true
            }
        }
        OutlinedTextField(
            name,
            {},
            Modifier.weight(1f),
            readOnly = true,
            label = { Text(stringResource(R.string.venue)) },
            singleLine = true,
            interactionSource = touches,
        )
        if (venue != null) {
            IconButton(onClick = { onVenue(null) }) {
                Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.cancel))
            }
        }
    }
    if (open) VenueChoiceDialog(teamId, onDismiss = { open = false }, onPick = {
        open = false
        onVenue(it)
    })
}

/** How many gyms there must be for a search to be worth the keyboard. */
private const val SEARCH_FROM = 8

@Composable
private fun VenueChoiceDialog(teamId: String, onDismiss: () -> Unit, onPick: (String) -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    var query by rememberSaveable { mutableStateOf("") }
    // a new gym: its name is written, the keyboard comes up only then
    var adding by rememberSaveable { mutableStateOf(false) }
    var failure by remember { mutableStateOf<String?>(null) }
    val all by produceState(emptyList<VenueRecord>(), teamId) {
        value = try {
            app.core { it.venueChoices(teamId, "") }
        } catch (_: MobileException) {
            emptyList()
        }
    }
    val found by produceState(emptyList<VenueRecord>(), teamId, query, all) {
        value = if (query.isBlank()) all else try {
            app.core { it.venueChoices(teamId, query) }
        } catch (_: MobileException) {
            emptyList()
        }
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.venue)) },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                if (adding || all.size >= SEARCH_FROM) {
                    OutlinedTextField(
                        query,
                        { query = it },
                        Modifier.fillMaxWidth(),
                        label = { Text(stringResource(R.string.venue_name)) },
                        singleLine = true,
                        isError = failure != null,
                        supportingText = noteOf(failure),
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words),
                    )
                }
                found.take(SEARCH_FROM).forEach { v ->
                    ListItem(
                        modifier = Modifier.clickable { onPick(v.id) },
                        headlineContent = { Text(v.name) },
                        supportingContent = v.address.takeIf { it.isNotBlank() }?.let { { Text(it) } },
                        trailingContent = if (v.pinned) {
                            { Icon(Icons.Filled.Star, contentDescription = null) }
                        } else null,
                    )
                }
                ListItem(
                    modifier = Modifier.clickable(enabled = !adding || query.isNotBlank()) {
                        if (!adding) {
                            adding = true
                        } else {
                            scope.launch {
                                try {
                                    onPick(app.core { it.createVenueNamed(query) }.id)
                                } catch (e: MobileException) {
                                    failure = e.reason
                                }
                            }
                        }
                    },
                    leadingContent = { Icon(Icons.Filled.Add, contentDescription = null) },
                    headlineContent = {
                        Text(if (adding) "${stringResource(R.string.venue_create)}: ${query.trim()}" else stringResource(R.string.venue_new))
                    },
                )
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

/** What a gym busy asks before saving: the clashes as the core tells them,
 * and the choice to save the same. */
@Composable
fun ClashDialog(clashes: List<VenueClashRecord>, onConfirm: () -> Unit, onDismiss: () -> Unit) = AlertDialog(
    onDismissRequest = onDismiss,
    icon = { Icon(Icons.Filled.Warning, contentDescription = null, tint = MaterialTheme.colorScheme.error) },
    title = { Text(stringResource(R.string.venue_busy_mark)) },
    text = {
        Column(Modifier.verticalScroll(rememberScrollState()), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            clashes.forEach { Text(it.message) }
        }
    },
    confirmButton = { TextButton(onClick = onConfirm) { Text(stringResource(R.string.venue_save_anyway)) } },
    dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
)

/** The gyms: the pinned first. A touch opens the page of the gym. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun VenuesScreen(onBack: () -> Unit, onVenue: (String) -> Unit, onNew: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val venues by produceState<List<VenueRecord>?>(null) {
        value = try {
            app.core { it.venues() }
        } catch (_: MobileException) {
            emptyList()
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.venues), Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
            )
        },
        floatingActionButton = {
            FloatingActionButton(onClick = onNew) {
                Icon(Icons.Filled.Add, contentDescription = stringResource(R.string.venue_new))
            }
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val list = venues) {
            null -> Centered(modifier) { CircularProgressIndicator() }
            else -> if (list.isEmpty()) Centered(modifier.padding(16.dp)) { Text(stringResource(R.string.venue_none)) }
            else LazyColumn(modifier, contentPadding = androidx.compose.foundation.layout.PaddingValues(bottom = 88.dp)) {
                items(list, key = { it.id }) { v -> VenueRow(v) { onVenue(v.id) } }
            }
        }
    }
}

@Composable
private fun VenueRow(v: VenueRecord, onClick: () -> Unit) = BoxRow(
    onClick = onClick,
    modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
    head = { Initials(v.initials) },
    tail = {
        if (v.pinned) {
            Icon(Icons.Filled.Star, contentDescription = stringResource(R.string.venue_pinned), Modifier.padding(end = 16.dp))
        }
    },
) {
    Text(v.name, style = MaterialTheme.typography.bodyLarge)
    if (v.address.isNotBlank()) {
        Text(v.address, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** A gym: its week, the gym of every team, the overlaps marked. Pin, map,
 * edit and delete in the bar. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun VenueScreen(id: String, onBack: () -> Unit, onEdit: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    var version by rememberSaveable { mutableIntStateOf(0) }
    var asking by rememberSaveable { mutableStateOf(false) }
    var day by rememberSaveable { mutableStateOf(app.clock.today()) }
    val venue by produceState<VenueRecord?>(null, id, version) {
        value = try {
            app.core { it.venue(id) }
        } catch (_: MobileException) {
            null
        }
    }
    val week by produceState(emptyList<VenueDay>(), id, day, version) {
        value = try {
            app.core { it.venueAgenda(id, AgendaSpan.WEEK, day.toString()) }
        } catch (_: MobileException) {
            emptyList()
        }
    }
    val v = venue
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(v?.name.orEmpty(), Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
                actions = {
                    if (v != null) {
                        IconButton(onClick = {
                            scope.launch {
                                try {
                                    app.core { it.setVenuePinned(id, !v.pinned) }
                                    version++
                                } catch (_: MobileException) {
                                }
                            }
                        }) {
                            Icon(
                                painterResource(if (v.pinned) R.drawable.ic_pin else R.drawable.ic_pin_outline),
                                contentDescription = stringResource(if (v.pinned) R.string.venue_unpin else R.string.venue_pin),
                            )
                        }
                        IconButton(onClick = {
                            scope.launch {
                                val (geo, web) = try {
                                    app.core { it.venueGeoUri(id) to it.venueWebUrl(id) }
                                } catch (_: MobileException) {
                                    null to null
                                }
                                openVenueMap(app, geo, web)
                            }
                        }) { Icon(Icons.Filled.Place, contentDescription = stringResource(R.string.venue_open_map)) }
                        MoreMenu { close ->
                            MenuEntry(stringResource(R.string.venue_edit), Icons.Filled.Edit, close, onEdit)
                            MenuEntry(stringResource(R.string.venue_delete), Icons.Filled.Delete, close) { asking = true }
                        }
                    }
                },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        if (v == null) Centered(modifier) { CircularProgressIndicator() }
        else LazyColumn(modifier) {
            val details = listOf(v.address, v.notes).filter { it.isNotBlank() }
            if (details.isNotEmpty()) {
                item(key = "details") {
                    Text(
                        details.joinToString("\n"),
                        Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                        style = MaterialTheme.typography.bodyMedium,
                    )
                }
            }
            item(key = "nav") {
                Row(Modifier.fillMaxWidth().padding(start = 16.dp, end = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                    Text(
                        weekRange(week.map { it.date }),
                        Modifier.weight(1f).title(),
                        style = MaterialTheme.typography.titleSmall,
                        color = MaterialTheme.colorScheme.primary,
                    )
                    PeriodButtons(
                        onPrevious = { day = day.minusWeeks(1) },
                        onToday = { day = app.clock.today() },
                        onNext = { day = day.plusWeeks(1) },
                    )
                }
            }
            // as in the agendas: the days with something and today, lit
            val today = app.clock.today()
            val shown = week.filter { it.occupations.isNotEmpty() || it.date == today.toString() }
            if (shown.isEmpty()) {
                item(key = "nothing") { NothingPlanned() }
            }
            shown.forEach { d ->
                val date = LocalDate.parse(d.date)
                if (date == today) {
                    item(key = "day/${d.date}") {
                        TodayBox {
                            DayHeader(date, true)
                            if (d.occupations.isEmpty()) NothingPlanned()
                            d.occupations.forEach { OccupationRow(it, lit = true) }
                        }
                    }
                } else {
                    item(key = "day/${d.date}") { DayHeader(date, false) }
                    items(d.occupations, key = { "${d.date}/${it.itemId}" }) { OccupationRow(it) }
                }
            }
        }
        if (asking) {
            AlertDialog(
                onDismissRequest = { asking = false },
                title = { Text(stringResource(R.string.venue_delete)) },
                text = { Text(stringResource(R.string.venue_delete_text, v?.name.orEmpty())) },
                confirmButton = {
                    TextButton(onClick = {
                        asking = false
                        scope.launch {
                            try {
                                app.core { it.removeVenue(id) }
                                onBack()
                            } catch (_: MobileException) {
                            }
                        }
                    }) { Text(stringResource(R.string.delete), color = MaterialTheme.colorScheme.error) }
                },
                dismissButton = { TextButton(onClick = { asking = false }) { Text(stringResource(R.string.cancel)) } },
            )
        }
    }
}

/** A session or a match in a gym: the hours, the teams, the clash. */
@Composable
private fun OccupationRow(o: io.github.naighes.scout4all.core.OccupationRecord, lit: Boolean = false) {
    val start = LocalDateTime.parse(o.start)
    val end = start.plusMinutes(o.minutes.toLong())
    val error = MaterialTheme.colorScheme.error
    ListItem(
        colors = if (lit) {
            androidx.compose.material3.ListItemDefaults.colors(containerColor = androidx.compose.ui.graphics.Color.Transparent)
        } else androidx.compose.material3.ListItemDefaults.colors(),
        overlineContent = { Text("${start.toLocalTime().format(HOUR_MINUTE)}–${end.toLocalTime().format(HOUR_MINUTE)}") },
        headlineContent = {
            Text(
                o.teams.joinToString(" · ") { it.name },
                color = if (o.overlapping) error else MaterialTheme.colorScheme.onSurface,
            )
        },
        supportingContent = if (o.overlapping) {
            { Text(stringResource(R.string.venue_busy_mark), color = error) }
        } else null,
        trailingContent = if (o.overlapping) {
            { Icon(Icons.Filled.Warning, contentDescription = stringResource(R.string.venue_busy_mark), tint = error) }
        } else null,
    )
}

/** The form of a gym, new (no `id`) or edited. The coordinates left empty
 * come from the link when it gives them. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun VenueFormScreen(id: String?, onBack: () -> Unit, onSaved: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val draft by produceState<VenueDraft?>(null, id) {
        value = try {
            app.core { if (id == null) it.newVenueDraft() else it.venueDraft(id) }
        } catch (_: MobileException) {
            null
        }
    }
    var changed by rememberSaveable { mutableStateOf(false) }
    val back = rememberFormBack(changed, onBack)
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(if (id == null) R.string.venue_new else R.string.venue_edit), Modifier.title()) },
                navigationIcon = { BackButton(back) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        val d = draft
        if (d == null) Centered(modifier) { CircularProgressIndicator() }
        else VenueFields(id, d, modifier, onChanged = { changed = true }, onSaved = onSaved)
    }
}

@Composable
private fun VenueFields(id: String?, draft: VenueDraft, modifier: Modifier, onChanged: () -> Unit, onSaved: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    var name by rememberSaveable { mutableStateOf(draft.name) }
    var address by rememberSaveable { mutableStateOf(draft.address) }
    var link by rememberSaveable { mutableStateOf(draft.mapUrl) }
    var latitude by rememberSaveable { mutableStateOf(draft.latitude) }
    var longitude by rememberSaveable { mutableStateOf(draft.longitude) }
    var notes by rememberSaveable { mutableStateOf(draft.notes) }
    var pinned by rememberSaveable { mutableStateOf(draft.pinned) }
    var failure by remember { mutableStateOf<Failure?>(null) }
    var busy by remember { mutableStateOf(false) }
    // the coordinates the link filled in: they follow it while it's written,
    // the ones written by hand stay
    var filled by remember { mutableStateOf<Pair<String, String>?>(null) }
    LaunchedEffect(link) {
        val free = (latitude.isBlank() && longitude.isBlank()) || filled == (latitude to longitude)
        if (free) {
            val found = try {
                app.core { it.venueLinkCoordinates(link) }
            } catch (_: MobileException) {
                null
            }?.let { it.latitude.toString() to it.longitude.toString() }
            latitude = found?.first.orEmpty()
            longitude = found?.second.orEmpty()
            filled = found
        }
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
            VenueText(R.string.venue_name, name, ErrorFor(failure, TrainingField.VENUE_NAME), KeyboardCapitalization.Words) { edit { name = it } }
            VenueText(R.string.venue_address, address, null, KeyboardCapitalization.Sentences) { edit { address = it } }
            VenueText(R.string.venue_map_link, link, ErrorFor(failure, TrainingField.MAP_LINK), KeyboardCapitalization.None) { edit { link = it } }
            VenueText(R.string.venue_latitude, latitude, ErrorFor(failure, TrainingField.COORDINATES), KeyboardCapitalization.None) { edit { latitude = it } }
            VenueText(R.string.venue_longitude, longitude, ErrorFor(failure, TrainingField.COORDINATES), KeyboardCapitalization.None) { edit { longitude = it } }
            VenueText(R.string.notes, notes, null, KeyboardCapitalization.Sentences) { edit { notes = it } }
            CheckRow(stringResource(R.string.venue_pinned), pinned, onChecked = { edit { pinned = it } })
        }
        FailureDialog(failure.elsewhere(TrainingField.VENUE_NAME, TrainingField.MAP_LINK, TrainingField.COORDINATES)) { failure = null }
        Button(
            onClick = {
                busy = true
                scope.launch {
                    failure = try {
                        app.core {
                            var lat = latitude
                            var lon = longitude
                            if (lat.isBlank() && lon.isBlank()) {
                                it.venueLinkCoordinates(link)?.let { c ->
                                    lat = c.latitude.toString()
                                    lon = c.longitude.toString()
                                }
                            }
                            it.saveVenue(id, VenueDraft(name, address, link, lat, lon, notes, pinned))
                        }
                        onSaved()
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
private fun VenueText(label: Int, value: String, error: String?, capitalization: KeyboardCapitalization, onChange: (String) -> Unit) =
    OutlinedTextField(
        value,
        onChange,
        Modifier.fillMaxWidth(),
        label = { Text(stringResource(label)) },
        singleLine = true,
        isError = error != null,
        supportingText = noteOf(error),
        keyboardOptions = KeyboardOptions(capitalization = capitalization, imeAction = ImeAction.Next),
    )
