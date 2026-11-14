package io.github.naighes.scout4all

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.interaction.PressInteraction
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.ui.res.painterResource
import androidx.compose.material.icons.outlined.Home
import androidx.compose.material.icons.filled.DateRange
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.naighes.scout4all.core.MatchField
import io.github.naighes.scout4all.core.CompetitionChoice
import io.github.naighes.scout4all.core.FormatChoice
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.MatchRoster
import io.github.naighes.scout4all.core.OpponentChoice
import io.github.naighes.scout4all.core.RoleChoice
import io.github.naighes.scout4all.core.RoleEnum
import io.github.naighes.scout4all.core.Stage
import io.github.naighes.scout4all.core.StageChoice
import io.github.naighes.scout4all.core.TeamListing
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.RosterPlayer
import java.time.DateTimeException
import java.time.Instant
import java.time.LocalDate
import java.time.OffsetDateTime
import java.time.ZoneId
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import java.time.temporal.ChronoUnit
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

sealed interface MatchFormState {
    data object Loading : MatchFormState
    data class Failed(val message: String) : MatchFormState
    data class Editing(
        val form: MatchForm,
        /** team A: the team the match is created from, or another one
         * chosen for a new match */
        val teamId: String,
        /** who can be on the score sheet, by number, and who is */
        val players: List<RosterPlayer> = emptyList(),
        /** the field the core rejected, with why */
        val invalid: Pair<MatchField, String>? = null,
        /** an error of the save */
        val error: String? = null,
        val busy: Boolean = false,
        /** the form as it was read: changed, going back asks first */
        val loaded: MatchForm = form,
        /** the second step, after the fields: who is on the score sheet */
        val onRoster: Boolean = false,
        /** the teams team A may be, for a new match */
        val teams: List<TeamListing.Team> = emptyList(),
        /** what team B may be, offered as it's written: the other teams,
         * then the names met */
        val opponents: List<OpponentChoice> = emptyList(),
        val competitions: List<CompetitionChoice> = emptyList(),
        val stages: List<StageChoice> = emptyList(),
        val formats: List<FormatChoice> = emptyList(),
        /** the roles of team A, to correct one on the score sheet */
        val roles: List<RoleChoice> = emptyList(),
        /** the number and the role of a player on the score sheet being
         * corrected */
        val correcting: RosterCorrection? = null,
    ) : MatchFormState {
        val changed: Boolean get() = form != loaded

        /** The form with team B and the competition linked to what's
         * written, as on the desktop: the one linked while it's still its
         * name, else the first with that name (the most recent, as
         * offered); a competition chosen gives its friendly. */
        fun linked(edited: MatchForm): MatchForm {
            val team = edited.opponentTeam
                ?.takeIf { id -> opponents.any { it.team == id && sameName(it.name, edited.opponent) } }
                ?: opponents.firstOrNull { it.team != null && sameName(it.name, edited.opponent) }?.team
            val competition = edited.competitionId
                ?.takeIf { id -> competitions.any { it.id == id && sameName(it.name, edited.competition) } }
                ?: competitions.firstOrNull { sameName(it.name, edited.competition) }?.id
            val chosen = competitions.firstOrNull { it.id == competition }?.takeIf { competition != form.competitionId }
            return edited.copy(
                opponentTeam = team,
                competitionId = competition,
                friendly = chosen?.friendly ?: edited.friendly,
            )
        }
    }
    /** saved, with its team A and its id: the screen goes on */
    data class Saved(val teamId: String, val matchId: String) : MatchFormState
}

/** A player of the score sheet whose number and role in this match are
 * being corrected, as written; why not, when the core says so. */
data class RosterCorrection(
    val player: RosterPlayer,
    val number: String,
    val role: RoleEnum?,
    val error: String? = null,
)

/** As the form offers team B: a team with its year. */
fun OpponentChoice.label(): String = year?.let { "$name ($it)" } ?: name

/** As the form offers a competition: with its season. */
fun CompetitionChoice.label(): String = season?.let { "$name ($it)" } ?: name

/** A new match of the team (`matchId` null), today, or one being edited.
 * The core checks the form. The `example` match is the one of the
 * tutorial, ready to save. */
class MatchFormViewModel(
    private val app: Scout4AllApplication,
    teamId: String,
    private val matchId: String?,
    example: Boolean = false,
) : ViewModel() {
    private val mutableState = MutableStateFlow<MatchFormState>(MatchFormState.Loading)
    val state: StateFlow<MatchFormState> = mutableState

    init {
        viewModelScope.launch {
            mutableState.value = try {
                app.core { core ->
                    val form = when (matchId) {
                        null -> when (example) {
                            true -> core.tutorialMatchForm(teamId, rfc3339(OffsetDateTime.now()))
                            false -> core.newMatchForm(rfc3339(OffsetDateTime.now()))
                        }
                        else -> core.matchForm(teamId, matchId)
                    }
                    MatchFormState.Editing(
                        form,
                        teamId,
                        core.matchRoster(teamId, matchId, form, null).players,
                        // A is chosen only for a new match
                        teams = when (matchId) {
                            null -> core.teams().filterIsInstance<TeamListing.Team>()
                            else -> emptyList()
                        },
                        opponents = core.opponentChoices(teamId),
                        competitions = core.competitions(),
                        stages = core.stages(),
                        formats = core.competitionFormats(),
                        roles = core.roles(teamId),
                    )
                }
            } catch (e: MobileException) {
                MatchFormState.Failed(e.reason)
            }
        }
    }

    private fun update(edit: (MatchFormState.Editing) -> MatchFormState.Editing) = mutableState.update { s ->
        when (s) {
            is MatchFormState.Editing -> edit(s)
            else -> s
        }
    }

    /** A change of the form: the error of the field changed, if any, goes
     * away; team B and the competition follow what's written. */
    fun change(field: MatchField?, edit: (MatchForm) -> MatchForm) = update { s ->
        s.copy(form = s.linked(edit(s.form)), invalid = s.invalid?.takeIf { it.first != field }, error = null)
    }

    /** Team B picked among the ones offered, by its label. */
    fun pickOpponent(label: String) {
        val editing = mutableState.value as? MatchFormState.Editing ?: return
        val choice = editing.opponents.firstOrNull { it.label() == label } ?: return
        change(MatchField.OPPONENT) { it.copy(opponent = choice.name, opponentTeam = choice.team) }
    }

    /** A competition picked among the ones offered, by its label. */
    fun pickCompetition(label: String) {
        val editing = mutableState.value as? MatchFormState.Editing ?: return
        val choice = editing.competitions.firstOrNull { it.label() == label } ?: return
        change(null) { it.copy(competition = choice.name, competitionId = choice.id) }
    }

    /** One change of the roster at a time, each on the one before. */
    private val toggling = Mutex()

    /** Another team A for the new match: its players on the score sheet,
     * the teams it may play. */
    fun chooseTeam(id: String) {
        viewModelScope.launch {
            toggling.withLock {
                val editing = mutableState.value as? MatchFormState.Editing ?: return@withLock
                if (editing.teamId == id) return@withLock
                try {
                    val (roster, opponents, roles) = app.core { core ->
                        val form = editing.form.copy(roster = null, numbers = emptyList())
                        Triple(core.matchRoster(id, matchId, form, null), core.opponentChoices(id), core.roles(id))
                    }
                    update { s ->
                        val chosen = s.copy(teamId = id, opponents = opponents, roles = roles, players = roster.players)
                        chosen.copy(
                            form = chosen.linked(s.form.copy(roster = roster.form.roster, numbers = roster.form.numbers)),
                            invalid = null,
                            error = null,
                        )
                    }
                } catch (e: MobileException) {
                    update { it.copy(error = e.reason) }
                }
            }
        }
    }

    /** Puts the player on the score sheet, or takes it off: the core
     * says who is on it then. */
    fun toggle(player: String) {
        viewModelScope.launch { toggling.withLock { toggleNow(player) } }
    }

    private suspend fun toggleNow(player: String) {
        val editing = mutableState.value as? MatchFormState.Editing ?: return
        val roster = try {
            app.core { it.matchRoster(editing.teamId, matchId, editing.form, player) }
        } catch (e: MobileException) {
            update { it.copy(error = e.reason) }
            return
        }
        update { s -> s.onRoster(roster) }
    }

    /** The form as changed meanwhile, the roster as the core gave it. */
    private fun MatchFormState.Editing.onRoster(roster: MatchRoster) = copy(
        form = form.copy(roster = roster.form.roster, numbers = roster.form.numbers),
        players = roster.players,
        invalid = invalid?.takeIf { it.first != MatchField.ROSTER },
        error = null,
    )

    /** The number and the role of the player in this match, to correct
     * them. */
    fun correcting(player: RosterPlayer?) = update { s ->
        s.copy(correcting = player?.let { RosterCorrection(it, it.number.toString(), it.roleValue) })
    }

    fun editCorrection(edit: (RosterCorrection) -> RosterCorrection) = update { s ->
        s.copy(correcting = s.correcting?.let { edit(it).copy(error = null) })
    }

    /** The number and the role as corrected, checked by the core: wrong,
     * the dialog stays with why. */
    fun correct() {
        viewModelScope.launch {
            toggling.withLock {
                val editing = mutableState.value as? MatchFormState.Editing ?: return@withLock
                val c = editing.correcting ?: return@withLock
                try {
                    val roster = app.core {
                        it.correctRosterNumber(editing.teamId, matchId, editing.form, c.player.id, c.number.trim(), c.role)
                    }
                    update { s -> s.onRoster(roster).copy(correcting = null) }
                } catch (e: MobileException) {
                    update { s -> s.copy(correcting = s.correcting?.copy(error = e.reason)) }
                }
            }
        }
    }

    /** The fields checked by the core, on to the roster. */
    fun next() {
        val editing = mutableState.value as? MatchFormState.Editing ?: return
        if (editing.busy) return
        mutableState.value = editing.copy(busy = true)
        viewModelScope.launch {
            mutableState.value = try {
                app.core { it.checkMatchFields(editing.teamId, matchId, editing.form) }
                editing.copy(onRoster = true, invalid = null, error = null)
            } catch (e: MobileException.InvalidMatch) {
                editing.copy(invalid = e.field to e.reason, error = null)
            } catch (e: MobileException) {
                editing.copy(error = e.reason, invalid = null)
            }
        }
    }

    /** Back from the roster to the fields, the roster as chosen. */
    fun toFields() = update { it.copy(onRoster = false, invalid = null, error = null) }

    /** Saves, once at a time. */
    fun save() {
        val editing = mutableState.value as? MatchFormState.Editing ?: return
        if (editing.busy) return
        mutableState.value = editing.copy(busy = true)
        viewModelScope.launch {
            mutableState.value = try {
                MatchFormState.Saved(editing.teamId, app.core { it.saveMatch(editing.teamId, matchId, editing.form) })
            } catch (e: MobileException.InvalidMatch) {
                // on the step of the field
                editing.copy(invalid = e.field to e.reason, error = null, onRoster = e.field == MatchField.ROSTER)
            } catch (e: MobileException) {
                editing.copy(error = e.reason, invalid = null)
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MatchFormScreen(
    teamId: String,
    matchId: String?,
    onBack: () -> Unit,
    onSaved: (teamId: String, matchId: String) -> Unit,
    example: Boolean = false,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val model = viewModel { MatchFormViewModel(app, teamId, matchId, example) }
    val state by model.state.collectAsState()
    val back = rememberFormBack((state as? MatchFormState.Editing)?.changed == true, onBack)
    val onRoster = (state as? MatchFormState.Editing)?.onRoster == true
    // from the roster, back to the fields
    BackHandler(enabled = onRoster) { model.toFields() }
    LaunchedEffect(state) {
        (state as? MatchFormState.Saved)?.let { onSaved(it.teamId, it.matchId) }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    val title = when {
                        onRoster -> R.string.roster_title
                        matchId == null -> R.string.new_match
                        else -> R.string.edit_match
                    }
                    Text(stringResource(title), Modifier.title())
                },
                navigationIcon = { BackButton(if (onRoster) model::toFields else back) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        when (val s = state) {
            MatchFormState.Loading, is MatchFormState.Saved -> Centered(modifier) { CircularProgressIndicator() }
            is MatchFormState.Failed -> Centered(modifier) { Text(s.message) }
            is MatchFormState.Editing -> {
                when (s.onRoster) {
                    true -> RosterContent(s, model, modifier)
                    false -> MatchFormContent(s, model, modifier)
                }
                s.correcting?.let { CorrectionDialog(it, s.roles, model) }
            }
        }
    }
}

/** The title of a part of the form. */
@Composable
private fun Section(title: Int) = Text(
    stringResource(title),
    Modifier.title(),
    style = MaterialTheme.typography.titleMedium,
    color = MaterialTheme.colorScheme.primary,
)

/** The fields, in two parts as on the desktop: the match (team A for a new
 * one, team B, date, court), then the competition and the rules. */
@Composable
private fun MatchFormContent(state: MatchFormState.Editing, model: MatchFormViewModel, modifier: Modifier) =
    Column(modifier) {
        val form = state.form
        val error = { field: MatchField -> state.invalid?.takeIf { it.first == field }?.second }
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).tutorial("match-fields").padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Section(R.string.match_section)
            if (state.teams.size > 1) {
                ChoiceField(
                    stringResource(R.string.team_a),
                    state.teams.map { it.id to "${it.name} (${it.year})" },
                    state.teamId,
                    onSelect = model::chooseTeam,
                )
            }
            val opponents = remember(state.opponents) { state.opponents.map { it.label() } }
            SuggestField(
                form.opponent,
                stringResource(R.string.opponent),
                opponents,
                error(MatchField.OPPONENT),
                onChange = { opponent -> model.change(MatchField.OPPONENT) { it.copy(opponent = opponent) } },
                onPick = model::pickOpponent,
            )
            // a name, not a team: it may become one
            if (form.opponent.isNotBlank() && form.opponentTeam == null) {
                CheckRow(stringResource(R.string.create_opponent), form.createOpponent, onChecked = { create ->
                    model.change(null) { it.copy(createOpponent = create) }
                })
            }
            DateField(form.date, error(MatchField.DATE)) { date ->
                model.change(MatchField.DATE) { it.copy(date = date) }
            }
            CourtChoice(form) { home, neutral -> model.change(null) { it.copy(home = home, neutral = neutral) } }
            Section(R.string.competition_section)
            val competitions = remember(state.competitions) { state.competitions.map { it.label() } }
            SuggestField(
                form.competition,
                stringResource(R.string.competition),
                competitions,
                error = null,
                onChange = { name -> model.change(null) { it.copy(competition = name) } },
                onPick = model::pickCompetition,
            )
            // a new one: of what format
            if (form.competition.isNotBlank() && form.competitionId == null) {
                ChoiceField(
                    stringResource(R.string.competition_format),
                    state.formats.map { it.value to it.name },
                    form.competitionFormat ?: state.formats.firstOrNull()?.value,
                    onSelect = { format -> model.change(null) { it.copy(competitionFormat = format) } },
                )
            }
            CheckRow(stringResource(R.string.friendly), form.friendly, onChecked = { friendly ->
                model.change(null) { it.copy(friendly = friendly) }
            })
            ChoiceField(
                stringResource(R.string.stage),
                listOf<Pair<Stage?, String>>(null to NO_CHOICE) + state.stages.map { it.value to it.name },
                form.stage,
                onSelect = { stage -> model.change(null) { it.copy(stage = stage) } },
            )
            OutlinedTextField(
                value = form.round,
                onValueChange = { round -> model.change(null) { it.copy(round = round) } },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.round)) },
                placeholder = { Text(stringResource(R.string.round_placeholder)) },
                singleLine = true,
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences, imeAction = ImeAction.Next),
            )
            OutlinedTextField(
                value = form.maxSubstitutions,
                onValueChange = { max ->
                    model.change(MatchField.MAX_SUBSTITUTIONS) { it.copy(maxSubstitutions = max) }
                },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.max_substitutions)) },
                singleLine = true,
                isError = error(MatchField.MAX_SUBSTITUTIONS) != null,
                supportingText = error(MatchField.MAX_SUBSTITUTIONS)?.let { { Text(it) } },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
            )
            NotesField(form.notes) { notes -> model.change(null) { it.copy(notes = notes) } }
        }
        state.error?.let {
            Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
        }
        // big and at the bottom, where the thumb is
        Button(
            onClick = model::next,
            enabled = !state.busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp).tutorial("match-next"),
        ) { Text(stringResource(R.string.go_on)) }
    }

/** Home, away or a neutral court, each with its icon. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CourtChoice(form: MatchForm, onChoose: (home: Boolean, neutral: Boolean) -> Unit) =
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
        val courts = listOf(
            Triple(true, false, R.string.home_match),
            Triple(false, false, R.string.away_match),
            Triple(false, true, R.string.neutral_game),
        )
        courts.forEachIndexed { index, (home, neutral, name) ->
            SegmentedButton(
                selected = form.neutral == neutral && (neutral || form.home == home),
                onClick = { onChoose(home, neutral) },
                shape = SegmentedButtonDefaults.itemShape(index, courts.size),
                // the icon of the team's list, not the check
                icon = {
                    when {
                        neutral -> Icon(painterResource(R.drawable.ic_ball), contentDescription = null, Modifier.size(18.dp))
                        home -> Icon(Icons.Outlined.Home, contentDescription = null)
                        else -> Icon(painterResource(R.drawable.ic_away), contentDescription = null)
                    }
                },
            ) { Text(stringResource(name), maxLines = 1) }
        }
    }

/** The second step: the players of the team as tiles, by number, the ones
 * on the score sheet lit; a tap puts one on or takes it off, who played in
 * a set stays; the pencil corrects the number and the role in this match.
 * Then the match is saved. */
@Composable
private fun RosterContent(state: MatchFormState.Editing, model: MatchFormViewModel, modifier: Modifier) =
    Column(modifier) {
        val chosen = state.players.count { it.chosen }
        val error = state.invalid?.takeIf { it.first == MatchField.ROSTER }?.second
        Column(Modifier.padding(horizontal = 16.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                stringResource(R.string.roster, chosen, MAX_ROSTER),
                Modifier.title(),
                style = MaterialTheme.typography.titleMedium,
                color = if (chosen > MAX_ROSTER) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface,
            )
            LinearProgressIndicator(
                progress = { (chosen.toFloat() / MAX_ROSTER).coerceAtMost(1f) },
                modifier = Modifier.fillMaxWidth(),
                color = if (chosen > MAX_ROSTER) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.primary,
            )
            Text(
                error ?: stringResource(R.string.roster_hint),
                style = MaterialTheme.typography.bodySmall,
                color = if (error != null) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        LazyVerticalGrid(
            // wider tiles with a larger font: long names stay whole
            columns = GridCells.Adaptive(minSize = 104.dp * LocalDensity.current.fontScale.coerceAtLeast(1f)),
            modifier = Modifier.weight(1f).tutorial("roster"),
            contentPadding = PaddingValues(16.dp),
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            items(state.players, key = { it.id }) { player ->
                PlayerTile(
                    player.number.toInt(),
                    player.name,
                    listOfNotNull(player.role, stringResource(R.string.played).takeIf { player.played }),
                    chosen = player.chosen,
                    locked = player.chosen && player.played,
                    onCorrect = { model.correcting(player) },
                ) { model.toggle(player.id) }
            }
        }
        state.error?.let {
            Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
        }
        Button(
            onClick = model::save,
            enabled = !state.busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp).tutorial("save-match"),
        ) { Text(stringResource(R.string.save)) }
    }

/** The number and the role of a player in this match, checked by the
 * core. */
@Composable
private fun CorrectionDialog(correction: RosterCorrection, roles: List<RoleChoice>, model: MatchFormViewModel) =
    NumberRoleDialog(
        "${correction.player.number} ${correction.player.name}",
        correction.number,
        correction.role,
        roles,
        correction.error,
        onNumber = { number -> model.editCorrection { it.copy(number = number) } },
        onRole = { role -> model.editCorrection { it.copy(role = role) } },
        onOk = model::correct,
        onCancel = { model.correcting(null) },
    )

/** The players a score sheet takes at most. */
private const val MAX_ROSTER = 14

/** The day of the match, in the language of the phone; a tap opens the
 * calendar. The time of day stays. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DateField(date: String?, error: String?, onDate: (String) -> Unit) {
    val parsed = date?.let(::parseDate)
    var picking by rememberSaveable { mutableStateOf(false) }
    // a read-only field doesn't take clicks: its presses do
    val presses = remember { MutableInteractionSource() }
    LaunchedEffect(presses) {
        presses.interactions.collect { if (it is PressInteraction.Release) picking = true }
    }
    OutlinedTextField(
        value = parsed?.format(DateTimeFormatter.ofLocalizedDate(FormatStyle.FULL)) ?: date.orEmpty(),
        onValueChange = {},
        modifier = Modifier.fillMaxWidth(),
        readOnly = true,
        label = { Text(stringResource(R.string.date)) },
        trailingIcon = { Icon(Icons.Filled.DateRange, contentDescription = null) },
        isError = error != null,
        supportingText = error?.let { { Text(it) } },
        interactionSource = presses,
    )
    if (picking) {
        // the picker counts days in UTC
        val picker = rememberDatePickerState(
            initialSelectedDateMillis = (parsed?.toLocalDate() ?: LocalDate.now())
                .atStartOfDay(ZoneOffset.UTC).toInstant().toEpochMilli(),
        )
        DatePickerDialog(
            onDismissRequest = { picking = false },
            confirmButton = {
                TextButton(onClick = {
                    picker.selectedDateMillis?.let { millis ->
                        val day = Instant.ofEpochMilli(millis).atZone(ZoneOffset.UTC).toLocalDate()
                        onDate(rfc3339(onDay(day, parsed)))
                    }
                    picking = false
                }) { Text(stringResource(R.string.ok)) }
            },
            dismissButton = {
                TextButton(onClick = { picking = false }) { Text(stringResource(R.string.cancel)) }
            },
        ) { DatePicker(picker) }
    }
}

private fun parseDate(date: String): OffsetDateTime? = try {
    OffsetDateTime.parse(date)
} catch (_: DateTimeException) {
    null
}

/** `day`, at the time of `date` (now without one), with the offset of the
 * phone on that day. */
private fun onDay(day: LocalDate, date: OffsetDateTime?): OffsetDateTime =
    day.atTime((date ?: OffsetDateTime.now()).toLocalTime())
        .atZone(ZoneId.systemDefault())
        .toOffsetDateTime()

/** What the core reads: RFC 3339, to the second. */
private fun rfc3339(date: OffsetDateTime): String =
    date.truncatedTo(ChronoUnit.SECONDS).format(DateTimeFormatter.ISO_OFFSET_DATE_TIME)
