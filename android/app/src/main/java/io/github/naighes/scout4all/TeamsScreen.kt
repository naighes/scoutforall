package io.github.naighes.scout4all

import android.app.Application
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.naighes.scout4all.core.Home
import io.github.naighes.scout4all.core.HomeMatch
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.NextSetKind
import io.github.naighes.scout4all.core.TeamListing
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

sealed interface TeamsState {
    data object Loading : TeamsState
    data class Loaded(val home: Home) : TeamsState
    data class Failed(val message: String) : TeamsState
}

/** The home, with the teams: the activity holds it, the splash waits for
 * it. */
class TeamsViewModel(application: Application) : AndroidViewModel(application) {
    private val mutableState = MutableStateFlow<TeamsState>(TeamsState.Loading)
    val state: StateFlow<TeamsState> = mutableState

    init {
        refresh()
    }

    /** Reads them again, showing the old ones meanwhile. */
    fun refresh() {
        viewModelScope.launch {
            mutableState.value = try {
                TeamsState.Loaded(getApplication<Scout4AllApplication>().core { it.home() })
            } catch (e: MobileException) {
                TeamsState.Failed(e.reason)
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TeamsScreen(
    teams: TeamsViewModel,
    onTeam: (String) -> Unit,
    onNewTeam: () -> Unit,
    onSettings: () -> Unit,
    onMatch: (teamId: String, matchId: String) -> Unit,
    onResume: (teamId: String, matchId: String, next: NextSetKind) -> Unit = { _, _, _ -> },
) {
    val state by teams.state.collectAsState()
    // back from a team, which may have changed
    LaunchedEffect(Unit) {
        if (state !is TeamsState.Loading) teams.refresh()
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.app_name), Modifier.title()) },
                actions = {
                    ImportButton(stringResource(R.string.import_team), teamId = null, onTeam = onTeam, onMatch = onMatch)
                    IconButton(onClick = onSettings) {
                        Icon(Icons.Filled.Settings, contentDescription = stringResource(R.string.settings))
                    }
                },
            )
        },
        floatingActionButton = {
            FloatingActionButton(onClick = onNewTeam) {
                Icon(Icons.Filled.Add, contentDescription = stringResource(R.string.new_team))
            }
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val s = state) {
            TeamsState.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is TeamsState.Failed -> Centered(modifier) { Text(s.message) }
            is TeamsState.Loaded -> when {
                s.home.teams.isEmpty() -> Centered(modifier) { Text(stringResource(R.string.no_teams)) }
                // room at the bottom: the button doesn't cover the last team
                else -> LazyColumn(modifier, contentPadding = PaddingValues(bottom = 88.dp)) {
                    // a section only with something in it
                    section(R.string.home_in_progress, s.home.inProgress) {
                        InProgressRow(it, { onMatch(it.teamId, it.id) }) { onResume(it.teamId, it.id, it.next) }
                    }
                    section(R.string.home_recent, s.home.recent) {
                        RecentRow(it) { onMatch(it.teamId, it.id) }
                    }
                    item { SectionTitle(R.string.teams) }
                    items(s.home.teams) { TeamRow(it, onTeam) }
                }
            }
        }
    }
}

private fun LazyListScope.section(title: Int, matches: List<HomeMatch>, row: @Composable (HomeMatch) -> Unit) {
    if (matches.isEmpty()) return
    item { SectionTitle(title) }
    items(matches, key = { "${it.teamId}/${it.id}" }) { row(it) }
}

@Composable
private fun SectionTitle(title: Int) = Text(
    stringResource(title),
    Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 4.dp).title(),
    style = MaterialTheme.typography.titleSmall,
    color = MaterialTheme.colorScheme.primary,
)

/** Against whom, of which team, when. */
@Composable
private fun MatchSupporting(m: HomeMatch) = Text("${m.teamName} · ${matchDate(m.date)}")

/** A match not over: touched, its page; the button goes on scouting it. */
@Composable
private fun InProgressRow(m: HomeMatch, onOpen: () -> Unit, onResume: () -> Unit) = ListItem(
    modifier = Modifier.clickable(onClick = onOpen),
    headlineContent = { Text(m.opponent) },
    supportingContent = { MatchSupporting(m) },
    trailingContent = {
        // not started yet: its first set
        val first = (m.next as? NextSetKind.Start)?.setNumber?.toInt() == 1
        FilledTonalButton(onClick = onResume) {
            Text(stringResource(if (first) R.string.start else R.string.resume))
        }
    },
)

/** A match over, with the sets won and lost. */
@Composable
private fun RecentRow(m: HomeMatch, onOpen: () -> Unit) = ListItem(
    modifier = Modifier.clickable(onClick = onOpen),
    headlineContent = { Text(m.opponent) },
    supportingContent = { MatchSupporting(m) },
    trailingContent = {
        Text(
            "${m.won}–${m.lost}",
            Modifier.readAs(stringResource(R.string.sets_score, m.won.toInt(), m.lost.toInt())),
            style = MaterialTheme.typography.titleLarge,
            fontWeight = FontWeight.SemiBold,
            color = resultColor(true, m.won.toInt(), m.lost.toInt()),
        )
    },
)

@Composable
private fun TeamRow(team: TeamListing, onTeam: (String) -> Unit) = when (team) {
    is TeamListing.Team -> ListItem(
        modifier = Modifier.clickable { onTeam(team.id) }.tutorial("team:${team.id}"),
        headlineContent = { Text(team.name) },
        supportingContent = {
            val players = team.players.toInt()
            Text("${team.year} · ${pluralStringResource(R.plurals.players, players, players)}")
        },
    )
    is TeamListing.Unreadable -> ListItem(
        headlineContent = { Text(stringResource(R.string.unreadable_team)) },
        supportingContent = { Text(team.folder) },
    )
}

@Composable
fun Centered(modifier: Modifier, content: @Composable () -> Unit) =
    Box(modifier, contentAlignment = Alignment.Center) { content() }
