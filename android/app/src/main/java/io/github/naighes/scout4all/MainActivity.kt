package io.github.naighes.scout4all

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
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
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Scout4All
import io.github.naighes.scout4all.core.TeamListing
import java.io.File
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        // the same layout as ~/.scout4all, in the sandbox of the app
        val dataDir = File(filesDir, "scout4all").apply { mkdirs() }
        enableEdgeToEdge()
        setContent {
            MaterialTheme {
                TeamsScreen(dataDir.path, Locale.getDefault().language)
            }
        }
    }
}

private sealed interface TeamsState {
    data object Loading : TeamsState
    data class Loaded(val teams: List<TeamListing>) : TeamsState
    data class Failed(val message: String) : TeamsState
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TeamsScreen(dataDir: String, language: String) {
    // the core blocks: never on the main thread
    val state by produceState<TeamsState>(TeamsState.Loading, dataDir, language) {
        value = withContext(Dispatchers.IO) {
            try {
                Scout4All(dataDir, language).use { TeamsState.Loaded(it.teams()) }
            } catch (e: MobileException) {
                TeamsState.Failed(e.message ?: "")
            }
        }
    }
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.teams)) }) }) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val s = state) {
            TeamsState.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is TeamsState.Failed -> Centered(modifier) { Text(s.message) }
            is TeamsState.Loaded -> when {
                s.teams.isEmpty() -> Centered(modifier) { Text(stringResource(R.string.no_teams)) }
                else -> LazyColumn(modifier) {
                    items(s.teams) { TeamRow(it) }
                }
            }
        }
    }
}

@Composable
private fun TeamRow(team: TeamListing) = when (team) {
    is TeamListing.Team -> ListItem(
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
private fun Centered(modifier: Modifier, content: @Composable () -> Unit) =
    Box(modifier, contentAlignment = Alignment.Center) { content() }
