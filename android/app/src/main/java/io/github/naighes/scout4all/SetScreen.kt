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
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.work.ExistingWorkPolicy
import io.github.naighes.scout4all.core.Correction
import io.github.naighes.scout4all.core.CorrectionState
import io.github.naighes.scout4all.core.EditField
import io.github.naighes.scout4all.core.EventTypeEnum
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.SetEvent
import io.github.naighes.scout4all.core.SetEvents
import kotlinx.coroutines.launch

private sealed interface SetState {
    data object Loading : SetState
    data class Loaded(val set: SetEvents) : SetState
    data class Failed(val message: String) : SetState
}

/** A set of a match: its events, the first first, with the score after
 * each; touch one to correct it. Its statistics on top. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SetScreen(teamId: String, matchId: String, setNumber: Int, onBack: () -> Unit, onStats: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    val snackbar = remember { SnackbarHostState() }
    // read again once an event is corrected
    var version by remember { mutableIntStateOf(0) }
    val state by produceState<SetState>(SetState.Loading, version) {
        value = try {
            SetState.Loaded(app.core { it.setEvents(teamId, matchId, setNumber.toUByte()) })
        } catch (e: MobileException) {
            SetState.Failed(e.reason)
        }
    }
    var correction by remember { mutableStateOf<Correction?>(null) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.set_n, setNumber)) },
                navigationIcon = { BackButton(onBack) },
                actions = { TextButton(onClick = onStats) { Text(stringResource(R.string.statistics)) } },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val s = state) {
            SetState.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is SetState.Failed -> Centered(modifier) { Text(s.message) }
            is SetState.Loaded -> SetEventsList(s.set, modifier) { event ->
                scope.launch {
                    try {
                        correction = app.core { it.correction(teamId, matchId, setNumber.toUByte(), event.index) }
                    } catch (e: MobileException) {
                        snackbar.showSnackbar(e.reason)
                    }
                }
            }
        }
    }
    correction?.let { c ->
        CorrectionSheet(
            c,
            onSave = {
                app.core { it.saveCorrection(c) }
                // a finished match is queued again
                app.sendUploads(ExistingWorkPolicy.APPEND_OR_REPLACE)
                correction = null
                version++
            },
            onDismiss = { correction = null },
        )
    }
}

@Composable
private fun SetEventsList(set: SetEvents, modifier: Modifier, onEvent: (SetEvent) -> Unit) {
    if (set.events.isEmpty()) {
        Centered(modifier) { Text(stringResource(R.string.no_events)) }
        return
    }
    // the last events first in sight: the ones just scouted
    val list = rememberLazyListState(initialFirstVisibleItemIndex = (set.events.size - 1).coerceAtLeast(0))
    LazyColumn(modifier, state = list, contentPadding = PaddingValues(vertical = 8.dp)) {
        items(set.events, key = { it.index.toInt() }) { EventRow(it) { onEvent(it) } }
    }
}

/** An event: its place, the symbol of the evaluation, the event and the
 * player, the set or serve; the score after it. The changes of the lineup
 * can't be corrected: dimmed. */
@Composable
private fun EventRow(e: SetEvent, onClick: () -> Unit) = Row(
    Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 10.dp),
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    val dim = MaterialTheme.colorScheme.onSurfaceVariant
    val color = if (e.editable) MaterialTheme.colorScheme.onSurface else dim
    Text(
        (e.index.toInt() + 1).toString(),
        Modifier.width(28.dp),
        style = MaterialTheme.typography.labelMedium,
        color = dim,
        textAlign = TextAlign.End,
    )
    Text(
        e.event.symbol ?: "",
        Modifier.width(20.dp),
        style = MaterialTheme.typography.titleMedium,
        fontWeight = FontWeight.Bold,
        color = color,
    )
    Column(Modifier.weight(1f)) {
        Text(
            listOfNotNull(e.event.name.replaceFirstChar { it.uppercase() }, e.event.technique).joinToString(" · "),
            style = MaterialTheme.typography.bodyLarge,
            color = color,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        e.event.player?.let {
            Text(
                "${it.number} ${it.name}",
                style = MaterialTheme.typography.labelMedium,
                color = dim,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
    Text(
        "${e.scoreUs}-${e.scoreThem}",
        style = MaterialTheme.typography.titleMedium,
        fontWeight = FontWeight.SemiBold,
        color = color,
    )
}

/** Correcting an event, from below: where it came in the set, then its
 * type, its evaluation and its set or serve; saved only if the events
 * after it still fit and the set ends as it did, else why not. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CorrectionSheet(correction: Correction, onSave: suspend () -> Unit, onDismiss: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    var state by remember { mutableStateOf<CorrectionState?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(correction) { state = app.core { correction.state() } }
    // every choice, then the event as it is now
    fun choose(choice: (Correction) -> Unit) = scope.launch {
        state = app.core {
            choice(correction)
            correction.state()
        }
        error = null
    }
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
    ) {
        val s = state ?: return@ModalBottomSheet
        Column(
            Modifier.fillMaxWidth().padding(horizontal = 16.dp).padding(bottom = 24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Text(
                stringResource(R.string.event_n, s.index.toInt() + 1),
                style = MaterialTheme.typography.titleLarge,
            )
            Text(
                listOf("${s.scoreUs}-${s.scoreThem}", s.rotation, s.phase).joinToString(" · "),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            s.event.player?.let { Text("${it.number} ${it.name}", style = MaterialTheme.typography.bodyLarge) }
            Row(
                Modifier.horizontalScroll(rememberScrollState()),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
            ) {
                s.eventTypes.forEach { e ->
                    FilterChip(
                        selected = e.value == s.event.event,
                        onClick = { choose { it.chooseEventType(e.value) } },
                        label = { Text(e.name.replaceFirstChar { it.uppercase() }) },
                    )
                }
            }
            if (EditField.EVAL in s.fields) {
                Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    s.evals.forEach { eval ->
                        val chosen = eval.value == s.eval
                        val modifier = Modifier.weight(1f).heightIn(min = 52.dp).semantics { contentDescription = eval.name }
                        val content = @Composable {
                            Text(eval.symbol, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
                        }
                        when {
                            chosen -> Button(
                                onClick = {},
                                modifier = modifier,
                                contentPadding = PaddingValues(0.dp),
                            ) { content() }
                            else -> OutlinedButton(
                                onClick = { choose { it.chooseEval(eval.value) } },
                                modifier = modifier,
                                contentPadding = PaddingValues(0.dp),
                            ) { content() }
                        }
                    }
                }
                s.evals.find { it.value == s.eval }?.let {
                    Text(it.name, style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            if (EditField.TECHNIQUE in s.fields) {
                Text(
                    stringResource(if (s.event.event == EventTypeEnum.S) R.string.serve_type else R.string.set_type),
                    style = MaterialTheme.typography.labelLarge,
                )
                Row(
                    Modifier.horizontalScroll(rememberScrollState()),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    FilterChip(
                        selected = s.techniques.none { it.chosen },
                        onClick = { choose { it.chooseTechnique(null) } },
                        label = { Text("–") },
                    )
                    s.techniques.forEach { t ->
                        FilterChip(
                            selected = t.chosen,
                            onClick = { choose { it.chooseTechnique(t.value) } },
                            label = { Text(listOfNotNull(t.code, t.shortName).joinToString(" ")) },
                        )
                    }
                }
            }
            error?.let { Text(it, color = Lost, style = MaterialTheme.typography.bodyMedium) }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(12.dp, Alignment.End)) {
                TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
                FilledTonalButton(onClick = {
                    scope.launch {
                        try {
                            onSave()
                        } catch (e: MobileException) {
                            error = e.reason
                        }
                    }
                }) { Text(stringResource(R.string.save)) }
            }
        }
    }
}
