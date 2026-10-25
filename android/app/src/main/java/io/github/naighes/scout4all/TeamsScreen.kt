package io.github.naighes.scout4all

import android.app.Application
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
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
import androidx.compose.ui.unit.dp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.TeamListing
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

sealed interface TeamsState {
    data object Loading : TeamsState
    data class Loaded(val teams: List<TeamListing>) : TeamsState
    data class Failed(val message: String) : TeamsState
}

/** The teams: the activity holds them, the splash waits for them. */
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
                TeamsState.Loaded(getApplication<Scout4AllApplication>().core { it.teams() })
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
) {
    val state by teams.state.collectAsState()
    // back from a team, which may have changed
    LaunchedEffect(Unit) {
        if (state !is TeamsState.Loading) teams.refresh()
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.teams), Modifier.title()) },
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
                s.teams.isEmpty() -> Centered(modifier) { Text(stringResource(R.string.no_teams)) }
                // room at the bottom: the button doesn't cover the last team
                else -> LazyColumn(modifier, contentPadding = PaddingValues(bottom = 88.dp)) {
                    items(s.teams) { TeamRow(it, onTeam) }
                }
            }
        }
    }
}

@Composable
private fun TeamRow(team: TeamListing, onTeam: (String) -> Unit) = when (team) {
    is TeamListing.Team -> ListItem(
        modifier = Modifier.clickable { onTeam(team.id) },
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
