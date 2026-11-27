package io.github.naighes.scout4all

import androidx.compose.ui.platform.testTag
import androidx.compose.ui.layout.ContentScale
import androidx.compose.foundation.Image
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Edit
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
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
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.layout.size
import io.github.naighes.scout4all.core.AgendaEventRecord
import io.github.naighes.scout4all.core.MatchListing
import io.github.naighes.scout4all.core.MatchStanding
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Player
import io.github.naighes.scout4all.core.TeamDetails
import io.github.naighes.scout4all.core.TeamRemovalCounts
import kotlinx.coroutines.launch
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.material.icons.filled.Delete
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
    onDvw: () -> Unit = {},
    onAnalysis: () -> Unit = {},
    onDeleted: () -> Unit = {},
    onNewSeason: () -> Unit = {},
    onTraining: () -> Unit = {},
    onSession: (String) -> Unit = {},
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val state by produceState<TeamState>(TeamState.Loading, id) {
        value = try {
            TeamState.Loaded(app.core { it.team(id) })
        } catch (e: MobileException) {
            TeamState.Failed(e.reason)
        }
    }
    // the folder of an unreadable match, to delete once confirmed; then
    // the matches read again
    var unreadable by rememberSaveable { mutableStateOf<String?>(null) }
    var deletedMatches by rememberSaveable { mutableIntStateOf(0) }
    // apart from the team: replaying the sets doesn't hold up the players
    val matches by produceState<MatchesState>(MatchesState.Loading, id, deletedMatches) {
        value = try {
            MatchesState.Loaded(app.core { it.matches(id) })
        } catch (e: MobileException) {
            MatchesState.Failed(e.reason)
        }
    }
    var tab by rememberSaveable { mutableIntStateOf(PLAYERS_TAB) }
    val appointments by rememberAppointments(id, matches)
    val loaded = state is TeamState.Loaded
    unreadable?.let { folder ->
        Deletion(
            stringResource(R.string.delete_match_text, folder),
            asking = true,
            onAsking = { if (!it) unreadable = null },
            delete = { it.deleteMatch(id, folder) },
            onDeleted = { deletedMatches++ },
        )
    }
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
                                onDvw = onDvw,
                            )
                        }
                        // the statistics of the matches together: nothing without matches
                        if ((matches as? MatchesState.Loaded)?.matches?.isNotEmpty() == true) {
                            IconButton(onClick = onAnalysis) {
                                Icon(painterResource(R.drawable.ic_stats), contentDescription = stringResource(R.string.analysis))
                            }
                        }
                        IconButton(onClick = onTraining) {
                            Icon(painterResource(R.drawable.ic_calendar), contentDescription = stringResource(R.string.tr_programming))
                        }
                        val name = (state as? TeamState.Loaded)?.team?.name.orEmpty()
                        val exporter = rememberExporter { it.exportTeam(id) }
                        val delete = rememberTeamDeletion(id, name, onDeleted)
                        MoreMenu { close ->
                            ExportEntries(exporter, close)
                            MenuEntry(stringResource(R.string.edit_team), Icons.Filled.Edit, close, onEdit)
                            MenuEntry(stringResource(R.string.new_season), Icons.Filled.Refresh, close, onNewSeason)
                            MenuEntry(stringResource(R.string.delete_team), Icons.Filled.Delete, close, delete)
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
            is TeamState.Failed -> LoadFailed(s.message, modifier)
            is TeamState.Loaded -> Column(modifier) {
                Row(
                    Modifier.padding(start = 16.dp, end = 16.dp, bottom = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(12.dp),
                ) {
                    TeamLogo(id, 48.dp)
                    Text(
                        listOfNotNull(s.team.year.toString(), s.team.classification, s.team.gender)
                            .joinToString(" · "),
                        style = MaterialTheme.typography.bodyLarge,
                    )
                }
                // what the team has today, over the tabs
                appointments?.today?.takeIf { it.isNotEmpty() }?.let { events ->
                    TodayAppointments(events, { _, m -> onMatch(m) }, { _, s -> onSession(s) })
                }
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
                    // the calendar is where the programming opens
                    else -> Matches(matches, appointments?.laterMatch, onTraining, onMatch, content, onUnreadable = { unreadable = it })
                }
            }
        }
    }
}

@Composable
private fun Players(team: TeamDetails, onPlayer: (String) -> Unit, modifier: Modifier) {
    when {
        team.players.isEmpty() -> Centered(modifier) {
            Text(stringResource(R.string.no_players), Modifier.padding(16.dp))
        }
        // room at the bottom: the button doesn't cover the last player
        else -> LazyColumn(modifier, contentPadding = PaddingValues(bottom = 88.dp)) {
            items(team.players, key = { it.id }) { p ->
                PortraitRow(
                    p.id, p.name, p.role, p.initials, p.number.toInt(), { onPlayer(p.id) },
                    Modifier.padding(horizontal = 12.dp, vertical = 4.dp),
                )
            }
        }
    }
}

@Composable
private fun Matches(
    state: MatchesState,
    next: AgendaEventRecord?,
    onCalendar: () -> Unit,
    onMatch: (String) -> Unit,
    modifier: Modifier,
    onUnreadable: (String) -> Unit,
) = when (state) {
    MatchesState.Loading -> Centered(modifier) { CircularProgressIndicator() }
    is MatchesState.Failed -> LoadFailed(state.message, modifier)
    // room at the bottom: the button doesn't cover the last match
    is MatchesState.Loaded -> LazyColumn(modifier, contentPadding = PaddingValues(top = 8.dp, bottom = 88.dp)) {
        // the next official match above the matches, when there is one
        next?.let { item(key = "next-match") { NextMatch(it, onCalendar) { _, m -> onMatch(m) } } }
        if (state.matches.isEmpty()) {
            item { Text(stringResource(R.string.no_matches), Modifier.padding(16.dp)) }
        } else {
            items(state.matches) { MatchRow(it, onMatch, onUnreadable) }
        }
    }
}

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun MatchRow(m: MatchListing, onMatch: (String) -> Unit, onUnreadable: (String) -> Unit) {
    val modifier = Modifier.padding(horizontal = 12.dp, vertical = 4.dp)
    val grey = MaterialTheme.colorScheme.onSurfaceVariant
    when (m) {
        is MatchListing.Match -> BoxRow(
            onClick = { onMatch(m.id) },
            modifier = modifier,
            head = { TeamPicture(m.opponentTeam, m.opponent) },
            // the sets of the team first, green when it won, red when it lost
            tail = {
                // nothing to show of a match not started
                if (m.standing != MatchStanding.TO_PLAY) {
                    TailFigure(
                        "${m.won}-${m.lost}",
                        resultColor(m.standing == MatchStanding.OVER, m.won.toInt(), m.lost.toInt()),
                        Modifier.readAs(stringResource(R.string.sets_score, m.won.toInt(), m.lost.toInt())),
                    )
                }
            },
        ) {
            Text(m.opponent, style = MaterialTheme.typography.bodyLarge)
            // the state under the date when both don't fit
            FlowRow(itemVerticalAlignment = Alignment.CenterVertically) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    // home or away at a glance
                    val place = stringResource(if (m.home) R.string.home else R.string.away)
                    // read as a text: a description would hide the rest of the row
                    val icon = Modifier.padding(end = 6.dp).size(18.dp).readAs(place)
                    when {
                        m.home -> Icon(Icons.Outlined.Home, contentDescription = null, icon, tint = grey)
                        else -> Icon(painterResource(R.drawable.ic_away), contentDescription = null, icon, tint = grey)
                    }
                    Text(matchDate(m.date), style = MaterialTheme.typography.bodyMedium, color = grey)
                }
                val state = when (m.standing) {
                    MatchStanding.TO_PLAY -> R.string.to_play to grey
                    MatchStanding.IN_PROGRESS -> R.string.in_progress to MaterialTheme.colorScheme.primary
                    MatchStanding.OVER -> null
                }
                state?.let { (label, color) ->
                    Row {
                        Text(" · ", Modifier.notRead(), style = MaterialTheme.typography.bodyMedium, color = grey)
                        Text(stringResource(label), style = MaterialTheme.typography.bodyMedium, color = color)
                    }
                }
            }
            // B of a match scouted by A: only its result here
            if (m.scoutedBy != null) {
                Text(stringResource(R.string.scouted_by_opponent), style = MaterialTheme.typography.bodyMedium, color = grey)
            }
        }
        // why, when it's known: e.g. a set this version doesn't replay
        // a touch deletes it, once confirmed
        is MatchListing.Unreadable -> BoxRow(
            onClick = { onUnreadable(m.folder) },
            modifier = modifier,
            head = { Icon(painterResource(Level.ERROR.icon), contentDescription = null, Modifier.size(32.dp), tint = Level.ERROR.color) },
        ) {
            Text(stringResource(R.string.unreadable_match), style = MaterialTheme.typography.bodyLarge)
            Text(
                listOfNotNull(m.folder, m.reason, stringResource(R.string.unreadable_match_delete)).joinToString("\n"),
                style = MaterialTheme.typography.bodyMedium,
                color = grey,
            )
        }
    }
}

/** The date in the language of the phone, as it was where the match was
 * played. */
fun matchDate(date: String): String = try {
    OffsetDateTime.parse(date).format(DateTimeFormatter.ofLocalizedDate(FormatStyle.MEDIUM))
} catch (_: java.time.DateTimeException) {
    date
}

/** A box of a list of players: the square portrait at its head (the
 * initials without a photo), the name and the role if any, the number at its
 * tail. The team's list, who comes in. */
@Composable
internal fun PortraitRow(
    id: String,
    name: String,
    role: String?,
    initials: String,
    number: Int,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) = BoxRow(
    onClick = onClick,
    modifier = modifier,
    head = { Portrait(id, initials) },
    tail = { TailFigure(number.toString(), MaterialTheme.colorScheme.primary) },
) {
    Text(name, style = MaterialTheme.typography.bodyLarge)
    role?.let {
        Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** The initials at the head of a `BoxRow`, in place of a missing picture. */
@Composable
internal fun Initials(initials: String) =
    Text(initials, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)

/** A transparent photo shows the grey of the box, an old one fills the
 * square. */
@Composable
private fun Portrait(id: String, initials: String) = when (val photo = photoOf(id)) {
    null -> Initials(initials)
    else -> Image(photo, contentDescription = null, Modifier.fillMaxSize().testTag(SHOWN_IMAGE), contentScale = ContentScale.Crop)
}

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

/** What deletes the team, once confirmed with what happens to its matches
 * (D16): the ones against a name go with it, the ones against another team
 * stay to that team. It is the call that starts the question. */
@Composable
private fun rememberTeamDeletion(id: String, name: String, onDeleted: () -> Unit): () -> Unit {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    var counts by remember { mutableStateOf<TeamRemovalCounts?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    val text = counts?.let { c ->
        val removed = stringResource(R.string.delete_team_text, name, c.removed.toInt())
        when (c.kept) {
            0u -> removed
            else -> "$removed ${stringResource(R.string.delete_team_kept, c.kept.toInt(), name)}"
        }
    }
    Deletion(
        text.orEmpty(),
        asking = counts != null,
        onAsking = { if (!it) counts = null },
        delete = { it.deleteTeam(id) },
        onDeleted = onDeleted,
    )
    error?.let { NoticeDialog(Notice.error(it)) { error = null } }
    return {
        scope.launch {
            try {
                counts = app.core { it.teamRemoval(id) }
            } catch (e: MobileException) {
                error = e.reason
            }
        }
        Unit
    }
}
