package io.github.naighes.scout4all

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.MatchDetails
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.NextSetKind
import io.github.naighes.scout4all.core.SetScore

private sealed interface MatchState {
    data object Loading : MatchState
    data class Loaded(val match: MatchDetails) : MatchState
    data class Failed(val message: String) : MatchState
}

/** A match, with its sets and the next one to start or resume. Read each
 * time it shows, as the match may have been edited or a set started. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MatchScreen(
    teamId: String,
    matchId: String,
    onBack: () -> Unit,
    onEdit: () -> Unit,
    onStartSet: () -> Unit,
    onResumeSet: () -> Unit,
    onSet: (set: Int) -> Unit,
    onStats: () -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val state by produceState<MatchState>(MatchState.Loading, teamId, matchId) {
        value = try {
            MatchState.Loaded(app.core { it.matchDetails(teamId, matchId) })
        } catch (e: MobileException) {
            MatchState.Failed(e.reason)
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text((state as? MatchState.Loaded)?.match?.opponent ?: "", Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
                actions = {
                    if (state is MatchState.Loaded) {
                        ExportButton(stringResource(R.string.export_match)) { it.exportMatch(teamId, matchId) }
                        IconButton(onClick = onEdit) {
                            Icon(Icons.Filled.Edit, contentDescription = stringResource(R.string.edit_match))
                        }
                    }
                },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val s = state) {
            MatchState.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is MatchState.Failed -> Centered(modifier) { Text(s.message) }
            is MatchState.Loaded -> MatchContent(s.match, onStartSet, onResumeSet, onSet, onStats, modifier)
        }
    }
}

@Composable
private fun MatchContent(
    m: MatchDetails,
    onStartSet: () -> Unit,
    onResumeSet: () -> Unit,
    onSet: (set: Int) -> Unit,
    onStats: () -> Unit,
    modifier: Modifier,
) = Column(modifier) {
    val place = stringResource(if (m.home) R.string.home else R.string.away)
    Text(
        "${matchDate(m.date)} · $place",
        modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp),
        style = MaterialTheme.typography.bodyLarge,
    )
    Text(
        "${m.won}-${m.lost}",
        modifier = Modifier.padding(horizontal = 16.dp).readAs(stringResource(R.string.sets_score, m.won.toInt(), m.lost.toInt())),
        style = MaterialTheme.typography.displayMedium,
        fontWeight = FontWeight.SemiBold,
        color = resultColor(m.finished, m.won.toInt(), m.lost.toInt()),
    )
    val inProgress = (m.next as? NextSetKind.Resume)?.setNumber
    // the statistics of the match under its score, a button of their own;
    // the ones of a set touching it
    if (m.sets.isNotEmpty()) {
        OutlinedButton(
            onClick = onStats,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp).heightIn(min = 48.dp),
            border = BorderStroke(1.dp, MaterialTheme.colorScheme.primary),
            colors = ButtonDefaults.outlinedButtonColors(contentColor = MaterialTheme.colorScheme.primary),
        ) {
            Icon(painterResource(R.drawable.ic_stats), contentDescription = null, Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
            Text(stringResource(R.string.match_stats), Modifier.weight(1f, fill = false))
        }
    }
    val sets = Modifier.weight(1f).fillMaxWidth()
    when {
        m.sets.isEmpty() && m.notes.isEmpty() -> Centered(sets) { Text(stringResource(R.string.no_sets)) }
        else -> LazyColumn(sets) {
            if (m.sets.isEmpty()) {
                item { Text(stringResource(R.string.no_sets), Modifier.padding(16.dp)) }
            }
            items(m.sets, key = { it.number.toInt() }) {
                SetRow(it, it.number == inProgress) { onSet(it.number.toInt()) }
            }
            // after the sets, whole: the list scrolls when they are long
            if (m.notes.isNotEmpty()) {
                item(key = "notes") { MatchNotes(m.notes) }
            }
        }
    }
    // big and at the bottom, where the thumb is
    val button = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp)
    when (val next = m.next) {
        is NextSetKind.Start -> Button(onClick = onStartSet, modifier = button) {
            Text(stringResource(R.string.start_set_n, next.setNumber.toInt()))
        }
        is NextSetKind.Resume -> Button(onClick = onResumeSet, modifier = button) {
            Text(stringResource(R.string.resume_set_n, next.setNumber.toInt()))
        }
        NextSetKind.Over -> {}
    }
}

@Composable
private fun MatchNotes(notes: String) = Column(Modifier.fillMaxWidth().padding(16.dp)) {
    Text(
        stringResource(R.string.notes),
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Text(notes, Modifier.padding(top = 4.dp), style = MaterialTheme.typography.bodyMedium)
}

@Composable
private fun SetRow(set: SetScore, inProgress: Boolean, onClick: () -> Unit) = ListItem(
    headlineContent = { Text(stringResource(R.string.set_n, set.number.toInt())) },
    supportingContent = if (inProgress) {
        { Text(stringResource(R.string.in_progress), color = MaterialTheme.colorScheme.primary) }
    } else {
        null
    },
    // the points of the team first, green when it won the set, red when it lost
    trailingContent = {
        Text(
            "${set.us}-${set.them}",
            Modifier.readAs(stringResource(R.string.score_to, set.us.toInt(), set.them.toInt())),
            style = MaterialTheme.typography.titleLarge,
            fontWeight = FontWeight.SemiBold,
            color = resultColor(!inProgress, set.us.toInt(), set.them.toInt()),
        )
    },
    modifier = Modifier.clickable(onClick = onClick),
)

@Composable
fun resultColor(over: Boolean, us: Int, them: Int) = when {
    !over -> MaterialTheme.colorScheme.onSurface
    us > them -> Won
    else -> Lost
}
