package io.github.naighes.scout4all

import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.animation.expandHorizontally
import androidx.compose.animation.shrinkHorizontally
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Surface
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.animation.Animatable
import androidx.compose.animation.Crossfade
import androidx.compose.animation.core.Animatable
import androidx.compose.animation.core.Spring
import androidx.compose.animation.core.spring
import androidx.compose.animation.core.tween
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.slideInVertically
import androidx.compose.animation.slideOutVertically
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.layout.size
import androidx.compose.material3.Icon
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.draw.scale
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.unit.sp
import androidx.work.ExistingWorkPolicy
import io.github.naighes.scout4all.core.EventTypeEnum
import io.github.naighes.scout4all.core.TeamSideEnum
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.naighes.scout4all.core.EvalChoice
import io.github.naighes.scout4all.core.DetailFilter
import io.github.naighes.scout4all.core.DetailStats
import io.github.naighes.scout4all.core.LiveStats
import io.github.naighes.scout4all.core.StatsScope
import io.github.naighes.scout4all.core.EventChoice
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Progress
import io.github.naighes.scout4all.core.Scouting
import io.github.naighes.scout4all.core.ScoutedEvent
import io.github.naighes.scout4all.core.ScoutingPlayer
import io.github.naighes.scout4all.core.ScoutingState
import io.github.naighes.scout4all.core.ScoutingStep
import io.github.naighes.scout4all.core.TechniqueChoice
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** All the screen shows at a step, read from the core at once. */
data class ScoutingView(
    val state: ScoutingState,
    /** the events of the opponent, the faults, the changes of the lineup */
    val otherEvents: List<EventChoice>,
    /** the events of the player chosen first, if more than one */
    val playerEvents: List<EventChoice>,
    val evals: List<EvalChoice>,
    val techniques: List<TechniqueChoice>,
    val replacements: List<ScoutingPlayer>,
)

sealed interface ScoutingUi {
    data object Loading : ScoutingUi
    data class Failed(val message: String) : ScoutingUi
    data class Choosing(
        val view: ScoutingView,
        /** a choice that didn't go, once */
        val error: String? = null,
        val busy: Boolean = false,
    ) : ScoutingUi
    /** the last point won the set */
    data class SetOver(val view: ScoutingView) : ScoutingUi
}

/** Scouting the set in progress, the player first: the core holds the
 * choices and saves every event once complete. */
class ScoutingViewModel(
    private val app: Scout4AllApplication,
    teamId: String,
    matchId: String,
) : ViewModel() {
    private val mutableState = MutableStateFlow<ScoutingUi>(ScoutingUi.Loading)
    val state: StateFlow<ScoutingUi> = mutableState
    private var scouting: Scouting? = null
    private val mutableStats = MutableStateFlow<LiveStats?>(null)
    /** the statistics of the scope last asked for */
    val stats: StateFlow<LiveStats?> = mutableStats

    init {
        viewModelScope.launch {
            mutableState.value = try {
                val s = app.core { it.scouting(teamId, matchId) }
                scouting = s
                ScoutingUi.Choosing(withContext(Dispatchers.IO) { view(s) })
            } catch (e: MobileException) {
                ScoutingUi.Failed(e.reason)
            }
        }
    }

    /** A player on court: the player first, or the player of the event. */
    fun player(id: String) = choose { s ->
        when (s.state().step) {
            ScoutingStep.PLAYER -> s.choosePlayer(id)
            else -> s.choosePlayerFirst(id)
        }
    }

    fun event(event: EventChoice) = choose { it.chooseEvent(event.value) }

    fun replacement(player: ScoutingPlayer) = choose { it.chooseReplacement(player.id) }

    fun technique(technique: TechniqueChoice) = choose {
        it.chooseTechnique(technique.value)
        Progress.CHOOSING
    }

    fun eval(eval: EvalChoice) = choose { it.chooseEval(eval.value) }

    fun undo() = choose {
        it.undo()
        Progress.CHOOSING
    }

    /** The statistics of a set or of the match, off the main thread. */
    fun stats(scope: StatsScope) {
        val s = scouting ?: return
        viewModelScope.launch {
            mutableStats.value = withContext(Dispatchers.IO) {
                try {
                    s.stats(scope)
                } catch (e: MobileException) {
                    null
                }
            }
        }
    }

    /** A fundamental in detail, off the main thread. */
    suspend fun detail(scope: StatsScope, event: EventTypeEnum, filter: DetailFilter): DetailStats? {
        val s = scouting ?: return null
        return withContext(Dispatchers.IO) {
            try {
                s.detail(scope, event, filter)
            } catch (e: MobileException) {
                null
            }
        }
    }

    fun errorShown() {
        val choosing = mutableState.value as? ScoutingUi.Choosing ?: return
        mutableState.value = choosing.copy(error = null)
    }

    /** One choice at a time: it may save an event, off the main thread.
     * Once the set is over, only the undo. */
    private fun choose(choice: (Scouting) -> Progress) {
        val s = scouting ?: return
        val view = when (val current = mutableState.value) {
            is ScoutingUi.Choosing -> current.view.takeUnless { current.busy } ?: return
            is ScoutingUi.SetOver -> current.view
            else -> return
        }
        mutableState.value = ScoutingUi.Choosing(view, busy = true)
        viewModelScope.launch {
            mutableState.value = withContext(Dispatchers.IO) {
                try {
                    when (choice(s)) {
                        Progress.SET_OVER -> {
                            // the match may be over, and queued
                            app.sendUploads(ExistingWorkPolicy.APPEND_OR_REPLACE)
                            ScoutingUi.SetOver(view(s))
                        }
                        else -> ScoutingUi.Choosing(view(s))
                    }
                } catch (e: MobileException) {
                    ScoutingUi.Choosing(view(s), error = e.reason)
                }
            }
        }
    }

    private fun view(s: Scouting): ScoutingView {
        val state = s.state()
        return ScoutingView(
            state = state,
            otherEvents = s.otherEvents(),
            playerEvents = state.player
                ?.takeIf { state.step == ScoutingStep.EVENT }
                ?.let { s.playerEvents(it.id) }
                .orEmpty(),
            evals = s.availableEvals(),
            techniques = s.availableTechniques(),
            replacements = s.replacementChoices(),
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ScoutingScreen(teamId: String, matchId: String, onBack: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val model = viewModel { ScoutingViewModel(app, teamId, matchId) }
    val state by model.state.collectAsState()
    val snackbar = remember { SnackbarHostState() }
    val error = (state as? ScoutingUi.Choosing)?.error
    LaunchedEffect(error) {
        if (error != null) {
            snackbar.showSnackbar(error)
            model.errorShown()
        }
    }
    val view = when (val s = state) {
        is ScoutingUi.Choosing -> s.view
        is ScoutingUi.SetOver -> s.view
        else -> null
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(view?.let { stringResource(R.string.set_n, it.state.setNumber.toInt()) } ?: "") },
                navigationIcon = { BackButton(onBack) },
            )
        },
        snackbarHost = { SnackbarHost(snackbar) },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val s = state) {
            ScoutingUi.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is ScoutingUi.Failed -> Centered(modifier) { Text(s.message) }
            is ScoutingUi.Choosing -> ScoutingContent(s.view, model, modifier)
            is ScoutingUi.SetOver -> {
                ScoutingContent(s.view, model, modifier)
                SetOverDialog(s.view.state, onUndo = model::undo, onDone = onBack)
            }
        }
    }
}

/** The score and the court always in sight; below them, or beside them in
 * landscape, the choices of the step and the undo. */
@Composable
private fun ScoutingContent(view: ScoutingView, model: ScoutingViewModel, modifier: Modifier) =
    BoxWithConstraints(modifier) {
        var statsOpen by rememberSaveable { mutableStateOf(false) }
        if (statsOpen) {
            val stats by model.stats.collectAsState()
            StatsSheet(
                stats = stats,
                currentSet = view.state.setNumber.toInt(),
                onScope = model::stats,
                loadDetail = model::detail,
                onDismiss = { statsOpen = false },
            )
        }
        val court = @Composable { m: Modifier ->
            Column(m.padding(vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
                Score(view.state)
                // where they stand: spread to receive, then where they
                // play once our serve is made or theirs received
                val players = view.state.court
                val touch = view.state.lastTouch
                val touched = touch?.player?.id
                // the serve waits for its evaluation: the server is in the
                // serve zone
                val serving = view.state.step == ScoutingStep.EVAL && view.state.event?.value == EventTypeEnum.S
                Court(
                    slot = { position ->
                        val player = players.getOrNull(position - 1)
                        CourtSlot(
                            number = player?.number?.toInt(),
                            name = player?.name,
                            setter = player?.setter == true,
                            libero = player?.libero == true,
                            highlighted = player != null && player.id == view.state.player?.id,
                            enabled = player?.selectable == true || (serving && position == 1),
                            serving = serving && position == 1,
                            x = player?.x,
                            y = player?.y,
                            key = player?.id,
                            touch = touch?.symbol.orEmpty().takeIf { player != null && player.id == touched },
                            spotlight = player?.spotlight == true,
                            efficiency = player?.efficiency,
                            grade = player?.grade,
                        )
                    },
                    onPosition = { position -> players.getOrNull(position - 1)?.let { model.player(it.id) } },
                    serveZone = true,
                    ball = CourtBall(
                        ours = view.state.ball == TeamSideEnum.US,
                        position = players.indexOfFirst { it.id == touched }.takeIf { it >= 0 }?.plus(1),
                        freeball = touch?.event == EventTypeEnum.FB || touch?.event == EventTypeEnum.OF,
                    ),
                )
            }
        }
        val panel = @Composable { m: Modifier ->
            Column(m) {
                HorizontalDivider()
                Column(
                    Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(12.dp),
                ) { Choices(view, model) }
                // always in the same place, where the thumb is: the undo on
                // the left, the statistics in the middle (swiping up the
                // row opens them too), the changes of the lineup on the
                // right edge
                Row(
                    Modifier.fillMaxWidth().swipeUp { statsOpen = true }
                        .padding(start = 8.dp, top = 4.dp, bottom = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    IconButton(onClick = model::undo, modifier = Modifier.size(56.dp)) {
                        Icon(
                            painterResource(R.drawable.ic_undo),
                            contentDescription = stringResource(R.string.undo),
                            modifier = Modifier.size(28.dp),
                        )
                    }
                    Box(Modifier.weight(1f), contentAlignment = Alignment.Center) {
                        StatsHandle(onOpen = { statsOpen = true })
                    }
                    LineupChanges(view.otherEvents.filter { it.value in lineupChanges }, model, Modifier)
                }
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

/** The teams and the points of the set, the home team on the left; the
 * ball by the team serving, the phase (break when we serve, side-out when
 * they do) and the rotation. A point stands out: the score turns, in green for ours, in
 * red for theirs, and the phone vibrates. */
@Composable
private fun Score(state: ScoutingState) {
    val weServe = state.servingTeam == TeamSideEnum.US
    val us = Side(state.team, state.scoreUs.toInt(), weServe, Won)
    val them = Side(state.opponent, state.scoreThem.toInt(), !weServe, Lost)
    val (left, right) = if (state.home) us to them else them to us
    val haptic = LocalHapticFeedback.current
    val points = state.scoreUs.toInt() + state.scoreThem.toInt()
    var previous by remember { mutableIntStateOf(points) }
    LaunchedEffect(points) {
        if (points > previous) haptic.performHapticFeedback(HapticFeedbackType.LongPress)
        previous = points
    }
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp), horizontalAlignment = Alignment.CenterHorizontally) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            TeamName(left, TextAlign.Start, Modifier.weight(1f))
            Points(left)
            Text(
                "–",
                modifier = Modifier.padding(horizontal = 8.dp),
                style = MaterialTheme.typography.headlineLarge,
            )
            Points(right)
            TeamName(right, TextAlign.End, Modifier.weight(1f))
        }
        Crossfade(weServe to state.rotation, label = "phase") { (serving, rotation) ->
            Text(
                stringResource(if (serving) R.string.break_phase else R.string.side_out_phase) + " · " + rotation,
                style = MaterialTheme.typography.labelLarge,
                fontWeight = FontWeight.Bold,
                letterSpacing = 2.sp,
                color = if (serving) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        LastEvent(state.events.firstOrNull())
    }
}

/** The last event saved, what undo takes back: the event, its evaluation
 * and its player. One line, kept also before the first. */
@Composable
private fun LastEvent(event: ScoutedEvent?) = AnimatedContent(
    event,
    transitionSpec = { fadeIn() togetherWith fadeOut() },
    label = "last event",
) { e ->
    Text(
        e?.let { listOfNotNull(listOfNotNull(it.name, it.symbol).joinToString(" "), it.player?.let { p -> "${p.number} · ${p.name}" }).joinToString(" · ") }
            .orEmpty(),
        modifier = Modifier.padding(top = 2.dp),
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
    )
}

private data class Side(val name: String, val points: Int, val serving: Boolean, val scored: Color)

/** The points of a team: a new one comes in from below and flashes in the
 * color of who scored; an undone one goes back. */
@Composable
private fun Points(side: Side) {
    val base = MaterialTheme.colorScheme.onSurface
    val color = remember { Animatable(base) }
    val scale = remember { Animatable(1f) }
    var previous by remember { mutableIntStateOf(side.points) }
    LaunchedEffect(side.points) {
        if (side.points > previous) {
            launch {
                scale.animateTo(1.35f, tween(120))
                scale.animateTo(1f, spring(dampingRatio = Spring.DampingRatioMediumBouncy))
            }
            color.snapTo(side.scored)
            color.animateTo(base, tween(1200))
        }
        previous = side.points
    }
    AnimatedContent(
        side.points,
        transitionSpec = {
            val up = targetState > initialState
            (slideInVertically { if (up) it else -it } + fadeIn()) togetherWith
                (slideOutVertically { if (up) -it else it } + fadeOut())
        },
        label = "points",
    ) { points ->
        Text(
            points.toString(),
            modifier = Modifier.scale(scale.value),
            style = MaterialTheme.typography.displaySmall,
            fontWeight = FontWeight.Bold,
            color = color.value,
        )
    }
}

@Composable
private fun TeamName(side: Side, align: TextAlign, modifier: Modifier) = Row(
    modifier,
    verticalAlignment = Alignment.CenterVertically,
    horizontalArrangement = if (align == TextAlign.End) Arrangement.End else Arrangement.Start,
) {
    val ball = @Composable {
        if (side.serving) {
            Icon(
                painterResource(R.drawable.ic_ball),
                contentDescription = stringResource(R.string.serving),
                modifier = Modifier.padding(horizontal = 4.dp).size(16.dp),
                tint = MaterialTheme.colorScheme.primary,
            )
        }
    }
    if (align == TextAlign.End) ball()
    Text(
        side.name,
        modifier = Modifier.weight(1f, fill = false),
        style = MaterialTheme.typography.titleMedium,
        textAlign = align,
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
    )
    if (align == TextAlign.Start) ball()
}

/** What the scout chooses now: the player on the court (with the other
 * events), the event of the player, the evaluation, the replacement. */
@Composable
private fun Choices(view: ScoutingView, model: ScoutingViewModel) {
    val state = view.state
    val player = state.player?.let { "${it.number} · ${it.name}" }
    when (state.step) {
        ScoutingStep.EVENT -> when (player) {
            null -> {
                Hint(stringResource(R.string.touch_the_player))
                RallyEvents(view.otherEvents, model)
            }
            else -> {
                Title(player)
                Buttons(view.playerEvents.map { it.name to { model.event(it) } })
            }
        }
        ScoutingStep.EVAL -> {
            // the serve shows on the court, with the server in the serve zone
            if (state.event?.value != EventTypeEnum.S) {
                Title(listOfNotNull(state.event?.name, player).joinToString(" · "))
            }
            if (view.techniques.isNotEmpty()) Techniques(view.techniques, model)
            Evals(view.evals, model)
            // the serve waits for its evaluation, or gives way
            RallyEvents(view.otherEvents, model)
        }
        ScoutingStep.PLAYER -> {
            Title(state.event?.name.orEmpty())
            Hint(stringResource(R.string.touch_the_player))
        }
        ScoutingStep.REPLACEMENT -> {
            Title(listOfNotNull(state.event?.name, player).joinToString(" · "))
            Hint(stringResource(R.string.who_comes_in))
            Column {
                view.replacements.forEach { p ->
                    ListItem(
                        modifier = Modifier.clickable { model.replacement(p) },
                        leadingContent = {
                            Text(
                                p.number.toString(),
                                style = MaterialTheme.typography.titleLarge,
                                fontWeight = FontWeight.SemiBold,
                            )
                        },
                        headlineContent = { Text(p.name) },
                        supportingContent = { Text(p.role) },
                    )
                }
            }
        }
    }
}

/** What is being scouted, on one line: the court shows the player too. */
@Composable
private fun Title(text: String) = Text(
    text,
    style = MaterialTheme.typography.titleMedium,
    maxLines = 1,
    overflow = TextOverflow.Ellipsis,
)

@Composable
private fun Hint(text: String) = Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant)

/** The changes of the lineup, rare and at stopped play, each with its icon
 * and a short name. */
private val lineupChanges = mapOf(
    EventTypeEnum.R to (R.drawable.ic_substitution to R.string.substitution_short),
    EventTypeEnum.CL to (R.drawable.ic_libero to R.string.libero),
    EventTypeEnum.CS to (R.drawable.ic_setter to R.string.setter_change_short),
)

/** The events of the rally the player isn't chosen first for, each with
 * its icon and a short name. */
private val rallyEvents = mapOf(
    EventTypeEnum.OS to (R.drawable.ic_their_point to R.string.their_point),
    EventTypeEnum.OE to (R.drawable.ic_their_error to R.string.their_error),
    EventTypeEnum.OF to (R.drawable.ic_their_freeball to R.string.their_freeball),
    EventTypeEnum.F to (R.drawable.ic_fault to R.string.fault),
)

/** The changes of the lineup: a half moon on the right edge, in the color
 * of the actions, opens them from right to left. Dimmed when there are none
 * to make now. */
@Composable
private fun LineupChanges(events: List<EventChoice>, model: ScoutingViewModel, modifier: Modifier) {
    var open by remember { mutableStateOf(false) }
    val expanded = open && events.isNotEmpty()
    Surface(
        modifier = modifier.alpha(if (events.isEmpty()) 0.38f else 1f),
        shape = RoundedCornerShape(topStartPercent = 50, bottomStartPercent = 50),
        color = MaterialTheme.colorScheme.primary,
        contentColor = MaterialTheme.colorScheme.onPrimary,
    ) {
        Row(Modifier.height(56.dp), verticalAlignment = Alignment.CenterVertically) {
            AnimatedVisibility(
                expanded,
                enter = expandHorizontally(expandFrom = Alignment.End) + fadeIn(),
                exit = shrinkHorizontally(shrinkTowards = Alignment.End) + fadeOut(),
            ) {
                Row(Modifier.padding(start = 12.dp)) {
                    events.forEach { event ->
                        val looks = lineupChanges[event.value]
                        Column(
                            Modifier
                                .clickable {
                                    open = false
                                    model.event(event)
                                }
                                .semantics(mergeDescendants = true) { contentDescription = event.name }
                                .padding(horizontal = 10.dp, vertical = 4.dp),
                            horizontalAlignment = Alignment.CenterHorizontally,
                        ) {
                            looks?.let { (icon, _) -> Icon(painterResource(icon), contentDescription = null, Modifier.size(24.dp)) }
                            Text(
                                looks?.let { (_, name) -> stringResource(name) } ?: event.name,
                                style = MaterialTheme.typography.labelSmall,
                                fontWeight = FontWeight.Bold,
                            )
                        }
                    }
                }
            }
            Box(
                Modifier.fillMaxHeight().clickable(enabled = events.isNotEmpty()) { open = !open }
                    .padding(start = 14.dp, end = 10.dp),
                contentAlignment = Alignment.Center,
            ) {
                Crossfade(expanded, label = "lineup") { shown ->
                    Icon(
                        if (shown) rememberVectorPainter(Icons.Default.Close) else painterResource(R.drawable.ic_lineup),
                        contentDescription = stringResource(R.string.lineup),
                    )
                }
            }
        }
    }
}

/** The other events of the rally in a row, never to scroll for: an icon
 * and a short name, the whole name for TalkBack. */
@Composable
private fun RallyEvents(events: List<EventChoice>, model: ScoutingViewModel) = Row(
    Modifier.fillMaxWidth().height(IntrinsicSize.Min),
    horizontalArrangement = Arrangement.spacedBy(8.dp),
) {
    events.filter { it.value !in lineupChanges }.forEach { event ->
        val looks = rallyEvents[event.value]
        FilledTonalButton(
            onClick = { model.event(event) },
            modifier = Modifier.weight(1f).fillMaxHeight().heightIn(min = 56.dp)
                .semantics { contentDescription = event.name },
            contentPadding = PaddingValues(horizontal = 4.dp, vertical = 6.dp),
            shape = MaterialTheme.shapes.medium,
        ) {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                looks?.let { (icon, _) -> Icon(painterResource(icon), contentDescription = null, Modifier.size(24.dp)) }
                Text(
                    looks?.let { (_, name) -> stringResource(name) } ?: event.name,
                    style = MaterialTheme.typography.labelSmall,
                    textAlign = TextAlign.Center,
                    maxLines = 2,
                )
            }
        }
    }
}

/** The evaluations in a row, by their symbol: the scouts know them. */
@Composable
private fun Evals(evals: List<EvalChoice>, model: ScoutingViewModel) = Row(
    horizontalArrangement = Arrangement.spacedBy(6.dp),
) {
    evals.forEach { eval ->
        Button(
            onClick = { model.eval(eval) },
            modifier = Modifier.weight(1f).heightIn(min = 52.dp)
                .semantics { contentDescription = eval.name },
            contentPadding = PaddingValues(0.dp),
        ) {
            Text(eval.symbol, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold)
        }
    }
}

/** The set or serve the evaluation saves, in a row, never to scroll for:
 * the code and the word for it, if any; the chosen one is filled. */
@Composable
private fun Techniques(techniques: List<TechniqueChoice>, model: ScoutingViewModel) = Row(
    horizontalArrangement = Arrangement.spacedBy(4.dp),
) {
    techniques.forEach { t ->
        val modifier = Modifier.weight(1f).height(56.dp).semantics { contentDescription = t.name }
        val padding = PaddingValues(0.dp)
        val content = @Composable {
            Column(horizontalAlignment = Alignment.CenterHorizontally) {
                Text(t.code, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.Bold)
                t.shortName?.let {
                    Text(it, style = MaterialTheme.typography.labelSmall, maxLines = 1, overflow = TextOverflow.Clip)
                }
            }
        }
        when {
            t.chosen -> FilledTonalButton(
                onClick = { model.technique(t) },
                modifier = modifier,
                contentPadding = padding,
                shape = MaterialTheme.shapes.medium,
            ) { content() }
            else -> OutlinedButton(
                onClick = { model.technique(t) },
                modifier = modifier,
                contentPadding = padding,
                shape = MaterialTheme.shapes.medium,
            ) { content() }
        }
    }
}

/** The events of the player, two by two, three by three if more than
 * four: never to scroll for. */
@Composable
private fun Buttons(buttons: List<Pair<String, () -> Unit>>) = Column(
    verticalArrangement = Arrangement.spacedBy(8.dp),
) {
    val columns = if (buttons.size > 4) 3 else 2
    buttons.chunked(columns).forEach { row ->
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            row.forEach { (name, onClick) ->
                Button(onClick = onClick, modifier = Modifier.weight(1f).heightIn(min = 64.dp)) {
                    Text(name, textAlign = TextAlign.Center)
                }
            }
            repeat(columns - row.size) { Column(Modifier.weight(1f)) {} }
        }
    }
}

/** The set is saved: back to the match, or undo the last point if it was a
 * mistake. */
@Composable
private fun SetOverDialog(state: ScoutingState, onUndo: () -> Unit, onDone: () -> Unit) = AlertDialog(
    onDismissRequest = {},
    title = {
        Text(
            stringResource(
                if (state.scoreUs > state.scoreThem) R.string.set_won else R.string.set_lost,
                state.setNumber.toInt(),
            ),
        )
    },
    text = { Text("${state.scoreUs}–${state.scoreThem}", style = MaterialTheme.typography.headlineMedium) },
    confirmButton = { TextButton(onClick = onDone) { Text(stringResource(R.string.ok)) } },
    dismissButton = { TextButton(onClick = onUndo) { Text(stringResource(R.string.undo)) } },
)
