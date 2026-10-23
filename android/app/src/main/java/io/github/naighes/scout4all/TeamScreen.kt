package io.github.naighes.scout4all

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.TeamDetails

private sealed interface TeamState {
    data object Loading : TeamState
    data class Loaded(val team: TeamDetails) : TeamState
    data class Failed(val message: String) : TeamState
}

/** A team: its matches will come here. Read each time it shows, as it may
 * have been edited. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TeamScreen(id: String, onBack: () -> Unit, onEdit: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val state by produceState<TeamState>(TeamState.Loading, id) {
        value = try {
            TeamState.Loaded(app.core { it.team(id) })
        } catch (e: MobileException) {
            TeamState.Failed(e.reason)
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text((state as? TeamState.Loaded)?.team?.name ?: "") },
                navigationIcon = { BackButton(onBack) },
                actions = {
                    if (state is TeamState.Loaded) {
                        IconButton(onClick = onEdit) {
                            Icon(Icons.Filled.Edit, contentDescription = stringResource(R.string.edit_team))
                        }
                    }
                },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val s = state) {
            TeamState.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is TeamState.Failed -> Centered(modifier) { Text(s.message) }
            is TeamState.Loaded -> TeamDetailsContent(s.team, modifier.padding(horizontal = 16.dp))
        }
    }
}

@Composable
private fun TeamDetailsContent(team: TeamDetails, modifier: Modifier) =
    Column(modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
        val players = team.players.toInt()
        Text(
            listOfNotNull(team.year.toString(), team.classification, team.gender).joinToString(" · "),
            style = MaterialTheme.typography.bodyLarge,
        )
        Text(
            pluralStringResource(R.plurals.players, players, players),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }

@Composable
fun BackButton(onBack: () -> Unit) = IconButton(onClick = onBack) {
    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back))
}
