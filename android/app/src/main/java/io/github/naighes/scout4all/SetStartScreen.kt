package io.github.naighes.scout4all

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.naighes.scout4all.core.Lineup
import io.github.naighes.scout4all.core.LineupPlayer
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.RoleEnum
import io.github.naighes.scout4all.core.SetStart
import io.github.naighes.scout4all.core.TeamSideEnum
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** What the scout is choosing a player for. */
sealed interface Pick {
    /** from 1 to 6 */
    data class Position(val position: Int) : Pick
    data object Setter : Pick
    data object Libero : Pick
    data object FallbackLibero : Pick
}

sealed interface SetStartState {
    data object Loading : SetStartState
    data class Failed(val message: String) : SetStartState
    data class Choosing(
        val lineup: Lineup,
        /** the open list of players, if any */
        val pick: Pick? = null,
        /** an error of the start */
        val error: String? = null,
        val busy: Boolean = false,
    ) : SetStartState
    /** saved: the screen goes back */
    data object Started : SetStartState
}

/** The start of the next set of a match: the core holds the choices and
 * checks them; the set is saved only at the start. */
class SetStartViewModel(
    private val app: Scout4AllApplication,
    teamId: String,
    matchId: String,
) : ViewModel() {
    private val mutableState = MutableStateFlow<SetStartState>(SetStartState.Loading)
    val state: StateFlow<SetStartState> = mutableState
    private var setStart: SetStart? = null

    init {
        viewModelScope.launch {
            mutableState.value = try {
                val start = app.core { it.setStart(teamId, matchId) }
                setStart = start
                SetStartState.Choosing(start.lineup())
            } catch (e: MobileException) {
                SetStartState.Failed(e.reason)
            }
        }
    }

    // the choices are in memory, in the core: no need to leave the main thread

    fun open(pick: Pick?) = mutableState.update { s ->
        when (s) {
            is SetStartState.Choosing -> s.copy(pick = pick)
            else -> s
        }
    }

    /** The players for `pick`, by number; the setters first for the setter,
     * the liberos first for the liberos. */
    fun choices(pick: Pick): List<LineupPlayer> {
        val start = setStart ?: return emptyList()
        return when (pick) {
            is Pick.Position -> start.positionChoices(pick.position.toUByte())
            Pick.Setter -> start.setterChoices().sortedByDescending { it.role == RoleEnum.SETTER }
            Pick.Libero -> start.liberoChoices().sortedByDescending { it.role == RoleEnum.LIBERO }
            Pick.FallbackLibero -> start.fallbackLiberoChoices().sortedByDescending { it.role == RoleEnum.LIBERO }
        }
    }

    fun serve(team: TeamSideEnum) = edit(null) { it.chooseServingTeam(team) }

    /** Chooses `player` (null to empty) for `pick`. Filling an empty choice
     * opens the next empty one: the lineup goes in at a glance. */
    fun choose(pick: Pick, player: String?) {
        val before = (mutableState.value as? SetStartState.Choosing)?.lineup ?: return
        val filled = player != null && chosen(before, pick) == null
        edit(pick) { start ->
            when (pick) {
                is Pick.Position -> start.place(pick.position.toUByte(), player)
                Pick.Setter -> start.chooseSetter(player)
                Pick.Libero -> start.chooseLibero(player)
                Pick.FallbackLibero -> start.chooseFallbackLibero(player)
            }
        }
        mutableState.update { s ->
            when (s) {
                is SetStartState.Choosing -> s.copy(pick = if (filled) missing(s.lineup) else null)
                else -> s
            }
        }
    }

    /** Saves the set, once at a time. */
    fun start() {
        val choosing = mutableState.value as? SetStartState.Choosing ?: return
        val start = setStart ?: return
        if (choosing.busy) return
        mutableState.value = choosing.copy(busy = true, error = null)
        viewModelScope.launch {
            mutableState.value = try {
                app.core { it.startSet(start) }
                SetStartState.Started
            } catch (e: MobileException) {
                choosing.copy(error = e.reason, busy = false)
            }
        }
    }

    private fun edit(pick: Pick?, change: (SetStart) -> Unit) {
        val start = setStart ?: return
        mutableState.update { s ->
            when (s) {
                is SetStartState.Choosing -> try {
                    change(start)
                    s.copy(lineup = start.lineup(), pick = pick, error = null)
                } catch (e: MobileException) {
                    s.copy(error = e.reason)
                }
                else -> s
            }
        }
    }
}

private fun chosen(lineup: Lineup, pick: Pick): LineupPlayer? = when (pick) {
    is Pick.Position -> lineup.positions.getOrNull(pick.position - 1)
    Pick.Setter -> lineup.setter
    Pick.Libero -> lineup.libero
    Pick.FallbackLibero -> lineup.fallbackLibero
}

/** The first choice still to make: the positions, the setter, the libero. */
private fun missing(lineup: Lineup): Pick? =
    ((1..6).map { Pick.Position(it) } + listOf(Pick.Setter, Pick.Libero))
        .firstOrNull { chosen(lineup, it) == null }

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SetStartScreen(teamId: String, matchId: String, onBack: () -> Unit, onStarted: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val model = viewModel { SetStartViewModel(app, teamId, matchId) }
    val state by model.state.collectAsState()
    LaunchedEffect(state) {
        if (state == SetStartState.Started) onStarted()
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text((state as? SetStartState.Choosing)?.let {
                        stringResource(R.string.start_set_n, it.lineup.setNumber.toInt())
                    } ?: "")
                },
                navigationIcon = { BackButton(onBack) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val s = state) {
            SetStartState.Loading, SetStartState.Started -> Centered(modifier) { CircularProgressIndicator() }
            is SetStartState.Failed -> Centered(modifier) { Text(s.message) }
            is SetStartState.Choosing -> SetStartContent(s, model, modifier)
        }
    }
}

/** The court stays in sight while choosing: the players to choose take the
 * place of the setter, the liberos and the button, below the court or, in
 * landscape, beside it. */
@Composable
private fun SetStartContent(state: SetStartState.Choosing, model: SetStartViewModel, modifier: Modifier) {
    val lineup = state.lineup
    val pick = state.pick
    // back closes the players to choose, before leaving the screen
    BackHandler(enabled = pick != null) { model.open(null) }
    BoxWithConstraints(modifier) {
        val court = @Composable { m: Modifier ->
            Column(m.padding(vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                ServingTeam(lineup, model)
                Court(lineup, pick, onPosition = { model.open(Pick.Position(it)) })
            }
        }
        val panel = @Composable { m: Modifier ->
            when (pick) {
                null -> Choices(state, model, m)
                else -> PlayerPicker(pick, lineup, model, m)
            }
        }
        when {
            maxWidth > maxHeight -> Row(Modifier.fillMaxSize()) {
                court(Modifier.weight(1f).fillMaxHeight().verticalScroll(rememberScrollState()))
                panel(Modifier.weight(1f).fillMaxHeight())
            }
            else -> Column(Modifier.fillMaxSize()) {
                court(Modifier.fillMaxWidth())
                panel(Modifier.weight(1f).fillMaxWidth())
            }
        }
    }
}

/** The setter, the liberos and the start of the set. */
@Composable
private fun Choices(state: SetStartState.Choosing, model: SetStartViewModel, modifier: Modifier) =
    Column(modifier) {
        val lineup = state.lineup
        Column(Modifier.weight(1f).verticalScroll(rememberScrollState())) {
            Choice(R.string.setter, lineup.setter) { model.open(Pick.Setter) }
            Choice(R.string.libero, lineup.libero) { model.open(Pick.Libero) }
            if (lineup.allowsFallbackLibero) {
                Choice(R.string.fallback_libero, lineup.fallbackLibero, optional = true) {
                    model.open(Pick.FallbackLibero)
                }
            }
        }
        state.error?.let {
            Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
        }
        // big and at the bottom, where the thumb is
        Button(
            onClick = model::start,
            enabled = lineup.complete && !state.busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
        ) { Text(stringResource(R.string.start_set)) }
    }

/** Who serves: chosen in the first set and in the tie-break, else as the
 * previous set says. */
@Composable
private fun ServingTeam(lineup: Lineup, model: SetStartViewModel) {
    val sides = listOf(TeamSideEnum.US to R.string.us, TeamSideEnum.THEM to R.string.them)
    Column(Modifier.padding(horizontal = 16.dp)) {
        Text(
            stringResource(R.string.serving_team),
            style = MaterialTheme.typography.labelLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(top = 4.dp)) {
            sides.forEachIndexed { index, (side, name) ->
                SegmentedButton(
                    selected = lineup.servingTeam == side,
                    onClick = { model.serve(side) },
                    enabled = lineup.choosesServingTeam,
                    shape = SegmentedButtonDefaults.itemShape(index, sides.size),
                ) { Text(stringResource(name)) }
            }
        }
    }
}

/** The court seen from the bench, the net at the top: front row 4 3 2,
 * back row 5 6 1. */
@Composable
private fun Court(lineup: Lineup, pick: Pick?, onPosition: (Int) -> Unit) = Column(
    Modifier.fillMaxWidth().padding(horizontal = 16.dp),
    verticalArrangement = Arrangement.spacedBy(8.dp),
) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
        Text(
            stringResource(R.string.net),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        HorizontalDivider(thickness = 2.dp, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
    listOf(listOf(4, 3, 2), listOf(5, 6, 1)).forEach { row ->
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            row.forEach { position ->
                val player = lineup.positions.getOrNull(position - 1)
                CourtCell(
                    position,
                    player,
                    setter = player != null && player.id == lineup.setter?.id,
                    picking = pick == Pick.Position(position),
                    modifier = Modifier.weight(1f),
                ) { onPosition(position) }
            }
        }
    }
}

@Composable
private fun CourtCell(
    position: Int,
    player: LineupPlayer?,
    setter: Boolean,
    picking: Boolean,
    modifier: Modifier,
    onClick: () -> Unit,
) = Surface(
    // a bit wider than tall: room for the players to choose
    modifier = modifier.aspectRatio(1.25f).clickable(onClick = onClick),
    shape = MaterialTheme.shapes.medium,
    color = MaterialTheme.colorScheme.surfaceVariant,
    // the position being chosen, in the color of the actions
    border = if (picking) BorderStroke(2.dp, MaterialTheme.colorScheme.primary) else null,
) {
    Box(Modifier.padding(8.dp)) {
        Text(
            stringResource(R.string.position_n, position),
            modifier = Modifier.align(Alignment.TopStart),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        if (setter) {
            Surface(
                modifier = Modifier.align(Alignment.TopEnd).width(20.dp).height(20.dp),
                shape = CircleShape,
                color = MaterialTheme.colorScheme.primary,
            ) {
                Box(contentAlignment = Alignment.Center) {
                    Text(
                        stringResource(R.string.setter_short),
                        style = MaterialTheme.typography.labelSmall,
                        fontWeight = FontWeight.Bold,
                        color = MaterialTheme.colorScheme.onPrimary,
                    )
                }
            }
        }
        when (player) {
            // an empty position asks for a player, in the color of the actions
            null -> Text(
                "+",
                modifier = Modifier.align(Alignment.Center),
                style = MaterialTheme.typography.headlineMedium,
                color = MaterialTheme.colorScheme.primary,
            )
            else -> Column(
                Modifier.align(Alignment.Center).padding(top = 8.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(
                    player.number.toString(),
                    style = MaterialTheme.typography.headlineMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    player.name,
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

/** The setter or a libero: the player chosen, or what to choose. */
@Composable
private fun Choice(name: Int, player: LineupPlayer?, optional: Boolean = false, onClick: () -> Unit) = ListItem(
    modifier = Modifier.clickable(onClick = onClick),
    headlineContent = { Text(stringResource(name)) },
    supportingContent = {
        when {
            player != null -> Text("${player.number} · ${player.name}")
            optional -> Text(stringResource(R.string.none))
            else -> Text(stringResource(R.string.choose), color = MaterialTheme.colorScheme.primary)
        }
    },
)

/** The players to choose for `pick`, scrolling on their own: the court
 * stays where it is. */
@Composable
private fun PlayerPicker(pick: Pick, lineup: Lineup, model: SetStartViewModel, modifier: Modifier) {
    val choices = remember(pick, lineup) { model.choices(pick) }
    val current = chosen(lineup, pick)
    Column(modifier) {
        HorizontalDivider()
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                when (pick) {
                    is Pick.Position -> stringResource(R.string.position_n, pick.position)
                    Pick.Setter -> stringResource(R.string.setter)
                    Pick.Libero -> stringResource(R.string.libero)
                    Pick.FallbackLibero -> stringResource(R.string.fallback_libero)
                },
                modifier = Modifier.weight(1f).padding(horizontal = 16.dp),
                style = MaterialTheme.typography.titleLarge,
            )
            IconButton(onClick = { model.open(null) }) {
                Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.close))
            }
        }
        LazyColumn(Modifier.weight(1f), contentPadding = PaddingValues(bottom = 16.dp)) {
            // a choice made can be undone; the fallback libero can stay out
            if (current != null || pick == Pick.FallbackLibero) {
                item {
                    ListItem(
                        modifier = Modifier.clickable { model.choose(pick, null) },
                        headlineContent = {
                            Text(stringResource(if (pick is Pick.Position) R.string.empty_position else R.string.none))
                        },
                    )
                }
            }
            if (choices.isEmpty()) {
                item {
                    Text(
                        stringResource(R.string.no_available_players),
                        modifier = Modifier.padding(16.dp),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
            }
            items(choices, key = { it.id }) { player ->
                ListItem(
                    modifier = Modifier.clickable { model.choose(pick, player.id) },
                    leadingContent = {
                        Box(Modifier.width(40.dp), contentAlignment = Alignment.CenterEnd) {
                            Text(
                                player.number.toString(),
                                style = MaterialTheme.typography.titleLarge,
                                fontWeight = FontWeight.SemiBold,
                                color = if (player.id == current?.id) {
                                    MaterialTheme.colorScheme.primary
                                } else {
                                    MaterialTheme.colorScheme.onSurface
                                },
                            )
                        }
                    },
                    headlineContent = { Text(player.name) },
                    supportingContent = player.roleName?.let { { Text(it) } },
                )
            }
        }
    }
}
