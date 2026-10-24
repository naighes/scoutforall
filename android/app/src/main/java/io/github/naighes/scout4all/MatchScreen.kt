package io.github.naighes.scout4all

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
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
    onStats: (set: Int?) -> Unit,
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
                title = { Text((state as? MatchState.Loaded)?.match?.opponent ?: "") },
                navigationIcon = { BackButton(onBack) },
                actions = {
                    if (state is MatchState.Loaded) {
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
            is MatchState.Loaded -> MatchContent(s.match, onStartSet, onResumeSet, onStats, modifier)
        }
    }
}

@Composable
private fun MatchContent(
    m: MatchDetails,
    onStartSet: () -> Unit,
    onResumeSet: () -> Unit,
    onStats: (set: Int?) -> Unit,
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
        modifier = Modifier.padding(horizontal = 16.dp),
        style = MaterialTheme.typography.displayMedium,
        fontWeight = FontWeight.SemiBold,
        color = resultColor(m.finished, m.won.toInt(), m.lost.toInt()),
    )
    val inProgress = (m.next as? NextSetKind.Resume)?.setNumber
    val sets = Modifier.weight(1f).fillMaxWidth()
    when {
        m.sets.isEmpty() -> Centered(sets) { Text(stringResource(R.string.no_sets)) }
        else -> LazyColumn(sets) {
            items(m.sets, key = { it.number.toInt() }) {
                SetRow(it, it.number == inProgress) { onStats(it.number.toInt()) }
            }
            // the statistics of a set touching it, of the match here
            item(key = "stats") {
                ListItem(
                    headlineContent = { Text(stringResource(R.string.match_stats)) },
                    trailingContent = { Icon(Icons.AutoMirrored.Filled.KeyboardArrowRight, contentDescription = null) },
                    modifier = Modifier.clickable { onStats(null) },
                )
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
            style = MaterialTheme.typography.titleLarge,
            fontWeight = FontWeight.SemiBold,
            color = resultColor(!inProgress, set.us.toInt(), set.them.toInt()),
        )
    },
    modifier = Modifier.clickable(onClick = onClick),
)

@Composable
private fun resultColor(over: Boolean, us: Int, them: Int) = when {
    !over -> MaterialTheme.colorScheme.onSurface
    us > them -> Won
    else -> Lost
}
