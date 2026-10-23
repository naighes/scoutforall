package io.github.naighes.scout4all

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledTonalButton
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.naighes.scout4all.core.EvalChoice
import io.github.naighes.scout4all.core.EventChoice
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Progress
import io.github.naighes.scout4all.core.Scouting
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
                        Progress.SET_OVER -> ScoutingUi.SetOver(view(s))
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
        val court = @Composable { m: Modifier ->
            Column(m.padding(vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
                Score(view.state)
                val players = view.state.court
                Court(
                    slot = { position ->
                        val player = players.getOrNull(position - 1)
                        CourtSlot(
                            number = player?.number?.toInt(),
                            name = player?.name,
                            setter = player?.setter == true,
                            libero = player?.libero == true,
                            highlighted = player != null && player.id == view.state.player?.id,
                            enabled = player?.selectable == true,
                        )
                    },
                    onPosition = { position -> players.getOrNull(position - 1)?.let { model.player(it.id) } },
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
                // always in the same place, where the thumb is
                OutlinedButton(
                    onClick = model::undo,
                    modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
                ) { Text(stringResource(R.string.undo)) }
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

/** The teams and the points of the set, the home team on the left. */
@Composable
private fun Score(state: ScoutingState) {
    val (left, right) = if (state.home) state.team to state.opponent else state.opponent to state.team
    val (leftPoints, rightPoints) = if (state.home) {
        state.scoreUs to state.scoreThem
    } else {
        state.scoreThem to state.scoreUs
    }
    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp), verticalAlignment = Alignment.CenterVertically) {
        TeamName(left, TextAlign.Start, Modifier.weight(1f))
        Text(
            "$leftPoints – $rightPoints",
            modifier = Modifier.padding(horizontal = 12.dp),
            style = MaterialTheme.typography.headlineLarge,
            fontWeight = FontWeight.SemiBold,
        )
        TeamName(right, TextAlign.End, Modifier.weight(1f))
    }
}

@Composable
private fun TeamName(name: String, align: TextAlign, modifier: Modifier) = Text(
    name,
    modifier = modifier,
    style = MaterialTheme.typography.titleMedium,
    textAlign = align,
    maxLines = 2,
    overflow = TextOverflow.Ellipsis,
)

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
                OtherEvents(view.otherEvents, model)
            }
            else -> {
                Title(player)
                Buttons(view.playerEvents.map { it.name to { model.event(it) } }, primary = true)
            }
        }
        ScoutingStep.EVAL -> {
            Title(listOfNotNull(state.event?.name, player).joinToString(" · "))
            if (view.techniques.isNotEmpty()) Techniques(view.techniques, model)
            Evals(view.evals, model)
            // the serve waits for its evaluation, or gives way
            OtherEvents(view.otherEvents, model)
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

@Composable
private fun Title(text: String) = Text(text, style = MaterialTheme.typography.titleLarge)

@Composable
private fun Hint(text: String) = Text(text, color = MaterialTheme.colorScheme.onSurfaceVariant)

@Composable
private fun OtherEvents(events: List<EventChoice>, model: ScoutingViewModel) {
    if (events.isEmpty()) return
    Text(
        stringResource(R.string.other_events),
        style = MaterialTheme.typography.labelLarge,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Buttons(events.map { it.name to { model.event(it) } }, primary = false)
}

/** The evaluations, big: the name, and what it means for the event. */
@Composable
private fun Evals(evals: List<EvalChoice>, model: ScoutingViewModel) = Column(
    verticalArrangement = Arrangement.spacedBy(8.dp),
) {
    evals.chunked(2).forEach { row ->
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            row.forEach { eval ->
                Button(
                    onClick = { model.eval(eval) },
                    modifier = Modifier.weight(1f).heightIn(min = 64.dp),
                ) {
                    Column(horizontalAlignment = Alignment.CenterHorizontally) {
                        Text(eval.name, textAlign = TextAlign.Center)
                        eval.description?.let {
                            Text(it, style = MaterialTheme.typography.labelSmall, textAlign = TextAlign.Center)
                        }
                    }
                }
            }
            // the last row keeps the width of the others
            if (row.size == 1) Column(Modifier.weight(1f)) {}
        }
    }
}

/** The set or serve the evaluation saves: the chosen one is marked. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Techniques(techniques: List<TechniqueChoice>, model: ScoutingViewModel) = FlowRow(
    horizontalArrangement = Arrangement.spacedBy(8.dp),
) {
    techniques.forEach { t ->
        FilterChip(selected = t.chosen, onClick = { model.technique(t) }, label = { Text(t.name) })
    }
}

/** Buttons two by two: the events of the player stand out, the others
 * don't. */
@Composable
private fun Buttons(buttons: List<Pair<String, () -> Unit>>, primary: Boolean) = Column(
    verticalArrangement = Arrangement.spacedBy(8.dp),
) {
    buttons.chunked(2).forEach { row ->
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            row.forEach { (name, onClick) ->
                val m = Modifier.weight(1f).heightIn(min = if (primary) 64.dp else 48.dp)
                when (primary) {
                    true -> Button(onClick = onClick, modifier = m) { Text(name, textAlign = TextAlign.Center) }
                    false -> FilledTonalButton(onClick = onClick, modifier = m) {
                        Text(name, textAlign = TextAlign.Center)
                    }
                }
            }
            if (row.size == 1) Column(Modifier.weight(1f)) {}
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
