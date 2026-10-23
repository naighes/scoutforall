package io.github.naighes.scout4all

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
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
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Player
import io.github.naighes.scout4all.core.TeamDetails

private sealed interface TeamState {
    data object Loading : TeamState
    data class Loaded(val team: TeamDetails) : TeamState
    data class Failed(val message: String) : TeamState
}

/** A team and its players: its matches will come here. Read each time it
 * shows, as the team or a player may have been edited. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TeamScreen(
    id: String,
    onBack: () -> Unit,
    onEdit: () -> Unit,
    onNewPlayer: () -> Unit,
    onPlayer: (String) -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val state by produceState<TeamState>(TeamState.Loading, id) {
        value = try {
            TeamState.Loaded(app.core { it.team(id) })
        } catch (e: MobileException) {
            TeamState.Failed(e.reason)
        }
    }
    val loaded = state is TeamState.Loaded
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text((state as? TeamState.Loaded)?.team?.name ?: "") },
                navigationIcon = { BackButton(onBack) },
                actions = {
                    if (loaded) {
                        IconButton(onClick = onEdit) {
                            Icon(Icons.Filled.Edit, contentDescription = stringResource(R.string.edit_team))
                        }
                    }
                },
            )
        },
        floatingActionButton = {
            if (loaded) {
                FloatingActionButton(onClick = onNewPlayer) {
                    Icon(Icons.Filled.Add, contentDescription = stringResource(R.string.new_player))
                }
            }
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val s = state) {
            TeamState.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is TeamState.Failed -> Centered(modifier) { Text(s.message) }
            is TeamState.Loaded -> TeamContent(s.team, onPlayer, modifier)
        }
    }
}

@Composable
private fun TeamContent(team: TeamDetails, onPlayer: (String) -> Unit, modifier: Modifier) =
    // room at the bottom: the button doesn't cover the last player
    LazyColumn(modifier, contentPadding = PaddingValues(bottom = 88.dp)) {
        item {
            Text(
                listOfNotNull(team.year.toString(), team.classification, team.gender).joinToString(" · "),
                modifier = Modifier.padding(horizontal = 16.dp),
                style = MaterialTheme.typography.bodyLarge,
            )
        }
        item {
            val players = team.players.size
            Text(
                pluralStringResource(R.plurals.players, players, players),
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 24.dp, bottom = 8.dp),
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        if (team.players.isEmpty()) {
            item {
                Text(
                    stringResource(R.string.no_players),
                    modifier = Modifier.padding(horizontal = 16.dp),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        items(team.players, key = { it.id }) { PlayerRow(it, onPlayer) }
    }

@Composable
private fun PlayerRow(player: Player, onPlayer: (String) -> Unit) = ListItem(
    modifier = Modifier.clickable { onPlayer(player.id) },
    leadingContent = {
        // the number first: it's what the scout looks for
        Box(Modifier.width(40.dp), contentAlignment = Alignment.CenterEnd) {
            Text(
                player.number.toString(),
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.SemiBold,
            )
        }
    },
    headlineContent = { Text(player.name) },
    supportingContent = player.role?.let { { Text(it) } },
)

@Composable
fun BackButton(onBack: () -> Unit) = IconButton(onClick = onBack) {
    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
}
