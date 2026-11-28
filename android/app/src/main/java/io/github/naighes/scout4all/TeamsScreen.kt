package io.github.naighes.scout4all

import android.app.Application
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.snapshotFlow
import kotlinx.coroutines.flow.first
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.draw.clip
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawWithContent
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.unit.LayoutDirection
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.relocation.BringIntoViewRequester
import androidx.compose.foundation.relocation.bringIntoViewRequester
import androidx.compose.material.icons.automirrored.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material3.OutlinedCard
import androidx.compose.material3.TextButton
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.withFrameNanos
import io.github.naighes.scout4all.core.HomeBlock
import io.github.naighes.scout4all.core.Scout4All
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.naighes.scout4all.core.Home
import io.github.naighes.scout4all.core.HomeMatch
import io.github.naighes.scout4all.core.MatchStanding
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

    /** Pins the team at the top of the list, or unpins it. */
    fun setPinned(teamId: String, pinned: Boolean) = change { it.setPinned(teamId, pinned) }

    private fun change(write: (Scout4All) -> Unit) {
        viewModelScope.launch {
            try {
                getApplication<Scout4AllApplication>().core { write(it) }
            } catch (e: MobileException) {
                mutableState.value = TeamsState.Failed(e.reason)
                return@launch
            }
            refresh()
        }
    }

    /** Closes a block of the home, or opens it again. */
    fun setClosed(block: HomeBlock, closed: Boolean) = change { it.setHomeBlockClosed(block, closed) }

    /** Shows again a team hidden from the home. */
    fun restore(teamId: String) = change { it.setHidden(teamId, false) }

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
    onDvw: (() -> Unit)? = null,
    onCompetitions: () -> Unit = {},
    onVenues: () -> Unit = {},
    onSession: (teamId: String, sessionId: String) -> Unit = { _, _ -> },
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
                    ImportButton(stringResource(R.string.import_team), teamId = null, onTeam = onTeam, onMatch = onMatch, onDvw = onDvw)
                    IconButton(onClick = onCompetitions) {
                        Icon(painterResource(R.drawable.ic_trophy), contentDescription = stringResource(R.string.competitions))
                    }
                    IconButton(onClick = onVenues) {
                        Icon(painterResource(R.drawable.ic_gym), contentDescription = stringResource(R.string.venues))
                    }
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
            is TeamsState.Failed -> LoadFailed(s.message, modifier)
            is TeamsState.Loaded -> when {
                s.home.teams.isEmpty() && s.home.hidden.isEmpty() -> Centered(modifier) { Text(stringResource(R.string.no_teams)) }
                else -> HomeBlocks(s.home, teams, modifier, onTeam, onMatch, onResume, onSession)
            }
        }
    }
}

/** The home as three blocks, a card each: the week, the matches and the
 * teams, each closed and opened by its title. */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun HomeBlocks(
    home: Home,
    teams: TeamsViewModel,
    modifier: Modifier,
    onTeam: (String) -> Unit,
    onMatch: (teamId: String, matchId: String) -> Unit,
    onResume: (teamId: String, matchId: String, next: NextSetKind) -> Unit,
    onSession: (teamId: String, sessionId: String) -> Unit,
) {
    val list = rememberLazyListState()
    // the tutorial asks for a team: its block open, and the team brought in
    // sight, below the matches
    val target = LocalTutorial.current?.step?.target?.takeIf { it.startsWith("team:") }
    val wanted = remember { BringIntoViewRequester() }
    LaunchedEffect(target) {
        if (target != null) {
            // once the list is measured
            val count = snapshotFlow { list.layoutInfo.totalItemsCount }.first { it > 0 }
            list.scrollToItem(count - 1)
            withFrameNanos {}
            wanted.bringIntoView()
        }
    }
    val clock = clock()
    val today = clock.today()
    var day by rememberSaveable { mutableStateOf(today) }
    val week by rememberHomeWeek(day, home)
    var showHidden by rememberSaveable { mutableStateOf(false) }
    // room at the bottom: the button doesn't cover the last team
    LazyColumn(modifier, list, contentPadding = PaddingValues(bottom = 88.dp)) {
        item(key = "week") {
            Block(R.string.home_calendar, R.drawable.ic_calendar, home.weekClosed, { teams.setClosed(HomeBlock.WEEK, it) }) {
                weekPieces(
                    week, today, withTeam = true,
                    onPrevious = { day = day.minusWeeks(1) },
                    onNext = { day = day.plusWeeks(1) },
                    onToday = { day = clock.today() },
                    onMatch = onMatch,
                    onSession = onSession,
                ).forEach { (id, piece) -> key(id) { piece() } }
            }
        }
        item(key = "matches") {
            Block(R.string.home_matches, R.drawable.ic_ball, home.matchesClosed, { teams.setClosed(HomeBlock.MATCHES, it) }) {
                if (home.inProgress.isEmpty() && home.recent.isEmpty()) {
                    Text(
                        stringResource(R.string.home_no_matches),
                        Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                // a section only with something in it
                section(R.string.home_in_progress, home.inProgress) {
                    InProgressRow(it, { onMatch(it.teamId, it.id) }) { onResume(it.teamId, it.id, it.next) }
                }
                section(R.string.home_recent, home.recent) {
                    RecentRow(it) { onMatch(it.teamId, it.id) }
                }
            }
        }
        item(key = "teams") {
            val closed = home.teamsClosed && target == null
            Block(R.string.teams, R.drawable.ic_all_teams, closed, { teams.setClosed(HomeBlock.TEAMS, it) }) {
                home.teams.forEach { team ->
                    val asked = team is TeamListing.Team && "team:${team.id}" == target
                    Box(if (asked) Modifier.bringIntoViewRequester(wanted) else Modifier) {
                        TeamRow(team, onTeam, teams::setPinned)
                    }
                }
                if (home.hidden.isNotEmpty()) {
                    TextButton(onClick = { showHidden = !showHidden }, Modifier.padding(horizontal = 12.dp)) {
                        Text(
                            if (showHidden) stringResource(R.string.hide_hidden)
                            else stringResource(R.string.show_hidden, home.hidden.size),
                        )
                    }
                    if (showHidden) {
                        home.hidden.forEach { if (it is TeamListing.Team) HiddenRow(it) { teams.restore(it.id) } }
                    }
                }
            }
        }
    }
}

/** A block of the home: its icon and title, which close and open it, then
 * its content while open. */
@Composable
private fun Block(
    title: Int,
    icon: Int,
    closed: Boolean,
    onClosed: (Boolean) -> Unit,
    content: @Composable ColumnScope.() -> Unit,
) = OutlinedCard(Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 4.dp)) {
    val action = stringResource(R.string.home_open_close)
    Row(
        Modifier
            .fillMaxWidth()
            .clickable(onClickLabel = action) { onClosed(!closed) }
            .padding(horizontal = 12.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Icon(painterResource(icon), contentDescription = null, Modifier.size(24.dp), tint = MaterialTheme.colorScheme.primary)
        Text(
            stringResource(title),
            Modifier.weight(1f).padding(start = 12.dp).title(),
            style = MaterialTheme.typography.titleMedium,
        )
        Icon(
            if (closed) Icons.AutoMirrored.Filled.KeyboardArrowRight else Icons.Filled.KeyboardArrowDown,
            contentDescription = null,
        )
    }
    if (!closed) {
        content()
        Spacer(Modifier.height(8.dp))
    }
}

/** A team hidden from the home: touched, it comes back. */
@Composable
private fun HiddenRow(team: TeamListing.Team, onRestore: () -> Unit) = BoxRow(
    onClick = onRestore,
    modifier = HOME_ROW,
    head = { TeamLogo(team.id, 48.dp) { Initials(team.initials) } },
) {
    Text(team.name, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
    Text(
        stringResource(R.string.show_in_home),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.primary,
    )
}

@Composable
private fun ColumnScope.section(title: Int, matches: List<HomeMatch>, row: @Composable (HomeMatch) -> Unit) {
    if (matches.isEmpty()) return
    SectionTitle(title)
    matches.forEach { key("${it.teamId}/${it.id}") { row(it) } }
}

@Composable
private fun SectionTitle(title: Int) = Text(
    stringResource(title),
    Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 4.dp).title(),
    style = MaterialTheme.typography.titleSmall,
    color = MaterialTheme.colorScheme.primary,
)

/** A match of the home: the logo of its team at the head, then against
 * whom, of which team and when. */
@Composable
private fun HomeMatchRow(
    m: HomeMatch,
    onOpen: () -> Unit,
    tail: @Composable () -> Unit = {},
    below: @Composable ColumnScope.() -> Unit = {},
) = BoxRow(
    onClick = onOpen,
    modifier = HOME_ROW,
    head = { TeamLogo(m.teamId, 48.dp) { Initials(m.teamInitials) } },
    tail = tail,
) {
    Text(m.opponent, style = MaterialTheme.typography.bodyLarge)
    val toPlay = stringResource(R.string.to_play).takeIf { m.standing == MatchStanding.TO_PLAY }
    Text(
        listOfNotNull(m.teamName, matchDate(m.date), toPlay).joinToString(" · "),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    below()
}

/** A match not over: touched, its page; the button goes on scouting it,
 * at the tail of the row, or under its texts where beside them it would
 * break the words of a name. */
@Composable
private fun InProgressRow(m: HomeMatch, onOpen: () -> Unit, onResume: () -> Unit) = BoxWithConstraints {
    // a larger font needs as much more room
    val narrow = maxWidth / LocalDensity.current.fontScale < BESIDE_WIDTH
    val button: @Composable (Modifier) -> Unit = { modifier ->
        FilledTonalButton(onClick = onResume, modifier) {
            Text(stringResource(if (m.standing == MatchStanding.TO_PLAY) R.string.start else R.string.resume))
        }
    }
    when {
        narrow -> HomeMatchRow(m, onOpen, below = { button(Modifier.align(Alignment.End)) })
        else -> HomeMatchRow(m, onOpen, tail = { button(Modifier.padding(end = 12.dp)) })
    }
}

/** The least width of a home row with a button beside its texts. */
private val BESIDE_WIDTH = 320.dp

/** A match over, with the sets won and lost. */
@Composable
private fun RecentRow(m: HomeMatch, onOpen: () -> Unit) = HomeMatchRow(m, onOpen) {
    TailFigure(
        "${m.won}-${m.lost}",
        resultColor(true, m.won.toInt(), m.lost.toInt()),
        Modifier.readAs(stringResource(R.string.sets_score, m.won.toInt(), m.lost.toInt())),
    )
}

@Composable
private fun TeamRow(
    team: TeamListing,
    onTeam: (String) -> Unit,
    onPin: (teamId: String, pinned: Boolean) -> Unit,
) = when (team) {
    is TeamListing.Team -> BoxRow(
        onClick = { onTeam(team.id) },
        // the bar clipped to the corners of the box
        modifier = HOME_ROW.tutorial("team:${team.id}").clip(ROW_SHAPE).pinnedRow(team.pinned),
        head = { TeamLogo(team.id, 48.dp) { Initials(team.initials) } },
        // full when pinned, at the top of the list: a touch changes it
        tail = {
            IconButton(onClick = { onPin(team.id, !team.pinned) }, Modifier.padding(end = 4.dp)) {
                Icon(
                    painterResource(if (team.pinned) R.drawable.ic_pin else R.drawable.ic_pin_outline),
                    contentDescription = stringResource(if (team.pinned) R.string.unpin_team else R.string.pin_team),
                    tint = if (team.pinned) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        },
    ) {
        val players = team.players.toInt()
        Text(team.name, style = MaterialTheme.typography.bodyLarge)
        Text(
            "${team.year} · ${pluralStringResource(R.plurals.players, players, players)}",
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
    is TeamListing.Unreadable -> BoxRow(
        onClick = null,
        modifier = HOME_ROW,
        head = { Icon(painterResource(Level.ERROR.icon), contentDescription = null, Modifier.size(32.dp), tint = Level.ERROR.color) },
    ) {
        Text(stringResource(R.string.unreadable_team), style = MaterialTheme.typography.bodyLarge)
        Text(team.folder, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

private val HOME_ROW = Modifier.padding(horizontal = 12.dp, vertical = 4.dp)

/** A pinned row: an amber bar along its start edge, above the box. */
@Composable
private fun Modifier.pinnedRow(pinned: Boolean): Modifier {
    if (!pinned) return this
    val bar = MaterialTheme.colorScheme.primary
    return drawWithContent {
        drawContent()
        val width = 4.dp.toPx()
        val x = if (layoutDirection == LayoutDirection.Ltr) 0f else size.width - width
        drawRect(bar, topLeft = Offset(x, 0f), size = Size(width, size.height))
    }
}

@Composable
fun Centered(modifier: Modifier, content: @Composable () -> Unit) =
    Box(modifier, contentAlignment = Alignment.Center) { content() }
