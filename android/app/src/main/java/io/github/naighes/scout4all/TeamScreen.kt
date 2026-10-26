package io.github.naighes.scout4all

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.MatchListing
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Player
import io.github.naighes.scout4all.core.TeamDetails
import java.time.OffsetDateTime
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle

private sealed interface TeamState {
    data object Loading : TeamState
    data class Loaded(val team: TeamDetails) : TeamState
    data class Failed(val message: String) : TeamState
}

private sealed interface MatchesState {
    data object Loading : MatchesState
    data class Loaded(val matches: List<MatchListing>) : MatchesState
    data class Failed(val message: String) : MatchesState
}

private const val PLAYERS_TAB = 0
private const val MATCHES_TAB = 1

/** A team, with its players and its matches in two tabs. Read each time it
 * shows, as the team or a player may have been edited. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TeamScreen(
    id: String,
    onBack: () -> Unit,
    onEdit: () -> Unit,
    onNewPlayer: () -> Unit,
    onPlayer: (String) -> Unit,
    onNewMatch: () -> Unit,
    onMatch: (String) -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val state by produceState<TeamState>(TeamState.Loading, id) {
        value = try {
            TeamState.Loaded(app.core { it.team(id) })
        } catch (e: MobileException) {
            TeamState.Failed(e.reason)
        }
    }
    // apart from the team: replaying the sets doesn't hold up the players
    val matches by produceState<MatchesState>(MatchesState.Loading, id) {
        value = try {
            MatchesState.Loaded(app.core { it.matches(id) })
        } catch (e: MobileException) {
            MatchesState.Failed(e.reason)
        }
    }
    var tab by rememberSaveable { mutableIntStateOf(PLAYERS_TAB) }
    val loaded = state is TeamState.Loaded
    Scaffold(
        topBar = {
            TopAppBar(
                // on one line, room for the buttons
                title = {
                    Text(
                        (state as? TeamState.Loaded)?.team?.name ?: "",
                        Modifier.title(),
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                },
                navigationIcon = { BackButton(onBack) },
                actions = {
                    if (loaded) {
                        if (tab == MATCHES_TAB) {
                            ImportButton(
                                stringResource(R.string.import_match),
                                teamId = id,
                                onTeam = {},
                                onMatch = { _, m -> onMatch(m) },
                            )
                        }
                        ExportButton(stringResource(R.string.export_team)) { it.exportTeam(id) }
                        IconButton(onClick = onEdit) {
                            Icon(Icons.Filled.Edit, contentDescription = stringResource(R.string.edit_team))
                        }
                    }
                },
            )
        },
        floatingActionButton = {
            if (loaded) {
                val (onNew, description) = when (tab) {
                    PLAYERS_TAB -> onNewPlayer to R.string.new_player
                    else -> onNewMatch to R.string.new_match
                }
                val target = if (tab == PLAYERS_TAB) "new-player" else "new-match"
                FloatingActionButton(onClick = onNew, modifier = Modifier.tutorial(target)) {
                    Icon(Icons.Filled.Add, contentDescription = stringResource(description))
                }
            }
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val s = state) {
            TeamState.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is TeamState.Failed -> Centered(modifier) { Text(s.message) }
            is TeamState.Loaded -> Column(modifier) {
                Text(
                    listOfNotNull(s.team.year.toString(), s.team.classification, s.team.gender)
                        .joinToString(" · "),
                    modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp),
                    style = MaterialTheme.typography.bodyLarge,
                )
                PrimaryTabRow(selectedTabIndex = tab) {
                    val players = s.team.players.size
                    Tab(
                        selected = tab == PLAYERS_TAB,
                        onClick = { tab = PLAYERS_TAB },
                        text = { Text(pluralStringResource(R.plurals.players, players, players)) },
                    )
                    Tab(
                        selected = tab == MATCHES_TAB,
                        onClick = { tab = MATCHES_TAB },
                        modifier = Modifier.tutorial("matches-tab"),
                        text = { Text(stringResource(R.string.matches)) },
                    )
                }
                val content = Modifier.fillMaxSize()
                when (tab) {
                    PLAYERS_TAB -> Players(s.team, onPlayer, content)
                    else -> Matches(matches, onMatch, content)
                }
            }
        }
    }
}

@Composable
private fun Players(team: TeamDetails, onPlayer: (String) -> Unit, modifier: Modifier) = when {
    team.players.isEmpty() -> Centered(modifier) { Text(stringResource(R.string.no_players)) }
    // room at the bottom: the button doesn't cover the last player
    else -> LazyColumn(modifier, contentPadding = PaddingValues(bottom = 88.dp)) {
        items(team.players, key = { it.id }) { PlayerRow(it, onPlayer) }
    }
}

@Composable
private fun Matches(state: MatchesState, onMatch: (String) -> Unit, modifier: Modifier) = when (state) {
    MatchesState.Loading -> Centered(modifier) { CircularProgressIndicator() }
    is MatchesState.Failed -> Centered(modifier) { Text(state.message) }
    is MatchesState.Loaded -> when {
        state.matches.isEmpty() -> Centered(modifier) { Text(stringResource(R.string.no_matches)) }
        // room at the bottom: the button doesn't cover the last match
        else -> LazyColumn(modifier, contentPadding = PaddingValues(bottom = 88.dp)) {
            items(state.matches) { MatchRow(it, onMatch) }
        }
    }
}

@Composable
private fun MatchRow(m: MatchListing, onMatch: (String) -> Unit) = when (m) {
    is MatchListing.Match -> ListItem(
        modifier = Modifier.clickable { onMatch(m.id) },
        // home or away at a glance, in the grey of the secondary text
        leadingContent = {
            val place = stringResource(if (m.home) R.string.home else R.string.away)
            // read as a text: a description would hide the rest of the row
            when {
                m.home -> Icon(Icons.Outlined.Home, contentDescription = null, Modifier.readAs(place))
                else -> Icon(painterResource(R.drawable.ic_away), contentDescription = null, Modifier.readAs(place))
            }
        },
        headlineContent = { Text(m.opponent) },
        supportingContent = {
            Row {
                Text(matchDate(m.date))
                if (!m.finished) {
                    Text(" · ", Modifier.notRead())
                    Text(stringResource(R.string.in_progress), color = MaterialTheme.colorScheme.primary)
                }
            }
        },
        // the sets of the team first, green when it won, red when it lost
        trailingContent = {
            Text(
                "${m.won}-${m.lost}",
                Modifier.readAs(stringResource(R.string.sets_score, m.won.toInt(), m.lost.toInt())),
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.SemiBold,
                color = when {
                    !m.finished -> MaterialTheme.colorScheme.onSurface
                    m.won > m.lost -> Won
                    else -> Lost
                },
            )
        },
    )
    is MatchListing.Unreadable -> ListItem(
        headlineContent = { Text(stringResource(R.string.unreadable_match)) },
        supportingContent = { Text(m.folder) },
    )
}

/** The date in the language of the phone, as it was where the match was
 * played. */
fun matchDate(date: String): String = try {
    OffsetDateTime.parse(date).format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM))
} catch (_: java.time.DateTimeException) {
    date
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

/** Back from a form, by the arrow (what it returns) or by the back of the
 * system: at once when nothing `changed`, else once the user agrees to
 * lose the changes. */
@Composable
fun rememberFormBack(changed: Boolean, onBack: () -> Unit): () -> Unit {
    var asking by rememberSaveable { mutableStateOf(false) }
    BackHandler(enabled = changed) { asking = true }
    if (asking) {
        AlertDialog(
            onDismissRequest = { asking = false },
            title = { Text(stringResource(R.string.discard_changes)) },
            confirmButton = {
                TextButton(onClick = {
                    asking = false
                    onBack()
                }) { Text(stringResource(R.string.discard)) }
            },
            dismissButton = {
                TextButton(onClick = { asking = false }) { Text(stringResource(R.string.keep_editing)) }
            },
        )
    }
    return { if (changed) asking = true else onBack() }
}
