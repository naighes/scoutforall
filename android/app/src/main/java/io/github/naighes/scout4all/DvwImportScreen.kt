package io.github.naighes.scout4all

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.work.ExistingWorkPolicy
import io.github.naighes.scout4all.core.ClassificationChoice
import io.github.naighes.scout4all.core.CountryChoice
import io.github.naighes.scout4all.core.DvwChoice
import io.github.naighes.scout4all.core.DvwOpponent
import io.github.naighes.scout4all.core.DvwPreview
import io.github.naighes.scout4all.core.DvwReady
import io.github.naighes.scout4all.core.DvwSide
import io.github.naighes.scout4all.core.DvwSimilarTeam
import io.github.naighes.scout4all.core.GenderChoice
import io.github.naighes.scout4all.core.GenderEnum
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.TeamClassificationEnum
import io.github.naighes.scout4all.core.TeamField
import java.time.Instant
import java.time.LocalDate
import java.time.Year
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** A DataVolley file chosen to import: kept by the app while its import
 * goes on, as the archives are (not in the saved state: too large); the
 * team it was chosen from, if any, offered first. */
class DvwOpened(val bytes: ByteArray, val fileName: String?, val teamId: String? = null)

/** Whether the file is a DataVolley one: by its name, as the TUI. */
fun isDvw(fileName: String?) = fileName?.endsWith(".dvw", ignoreCase = true) == true

/** What happened to a team of the file: imported (its id) or why not. */
data class DvwResult(val team: String, val message: String, val teamId: String? = null)

/** The steps of the import, as the TUI: the teams to import, the day when
 * the file can't tell it, the substitutions, then for each team ours (a
 * similar one or a new one), team B (a similar one, only its name, or a
 * new one with the players of the file), the check, the save. */
sealed interface DvwStep {
    data object Loading : DvwStep
    data class Failed(val message: String) : DvwStep
    data object Sides : DvwStep
    data object Day : DvwStep
    data object Substitutions : DvwStep
    data class Team(val similar: List<DvwSimilarTeam>) : DvwStep
    data class NewTeam(val similar: List<DvwSimilarTeam>) : DvwStep
    /** team B: ours like it, then only its name, then a new team; team A
     * as chosen */
    data class Opponent(val similar: List<DvwSimilarTeam>, val choice: DvwChoice) : DvwStep
    data class Ready(val ready: DvwReady, val choice: DvwChoice) : DvwStep
    data object Done : DvwStep
}

data class DvwState(
    val step: DvwStep = DvwStep.Loading,
    val preview: DvwPreview? = null,
    /** the option of a list chosen */
    val selected: Int = 0,
    val sides: List<DvwSide> = emptyList(),
    /** the side being imported, of `sides` */
    val current: Int = 0,
    /** `YYYY-MM-DD` */
    val day: String? = null,
    val substitutions: String = "",
    val classifications: List<ClassificationChoice> = emptyList(),
    val classification: TeamClassificationEnum? = null,
    val genders: List<GenderChoice> = emptyList(),
    val gender: GenderEnum? = null,
    val country: CountryChoice? = null,
    /** a field of the new team missing, said next to it as in the form */
    val invalid: Pair<TeamField, String>? = null,
    /** why team B as chosen can't be: another one may do */
    val opponentError: String? = null,
    val results: List<DvwResult> = emptyList(),
    val working: Boolean = false,
    /** the side like the team the import was chosen from, of the sides */
    val fromSide: Int = 0,
) {
    val side: DvwSide? get() = sides.getOrNull(current)
}

class DvwImportViewModel(private val app: Scout4AllApplication, private val file: DvwOpened?) : ViewModel() {
    private val mutableState = MutableStateFlow(DvwState())
    val state: StateFlow<DvwState> = mutableState
    private val bytes get() = file?.bytes ?: ByteArray(0)

    init {
        viewModelScope.launch {
            mutableState.value = try {
                app.core { core ->
                    val preview = core.previewDvw(bytes)
                    val form = core.newTeamForm(Year.now().value.toUShort())
                    // from a team: the side like it to start, the home one
                    // when neither
                    val like = { side: DvwSide ->
                        file?.teamId?.let { id -> core.dvwSimilarTeams(bytes, side, null).any { it.id == id } } == true
                    }
                    val fromSide = if (!like(DvwSide.HOME) && like(DvwSide.VISITING)) 1 else 0
                    DvwState(
                        step = DvwStep.Sides,
                        selected = fromSide,
                        fromSide = fromSide,
                        preview = preview,
                        classifications = core.classifications(),
                        genders = core.genders(),
                        country = form.country?.let { CountryChoice(it, core.countryName(it)) },
                    )
                }
            } catch (e: MobileException) {
                DvwState(step = DvwStep.Failed(e.reason))
            }
        }
    }

    fun select(index: Int) = mutableState.update { it.copy(selected = index, opponentError = null) }
    fun day(day: String) = mutableState.update { it.copy(day = day) }
    fun substitutions(text: String) = mutableState.update { it.copy(substitutions = text.filter(Char::isDigit).take(2)) }
    fun classification(classification: TeamClassificationEnum) =
        mutableState.update { it.copy(classification = classification, invalid = null) }
    fun gender(gender: GenderEnum) = mutableState.update { it.copy(gender = gender, invalid = null) }
    fun country(country: CountryChoice) = mutableState.update { it.copy(country = country, invalid = null) }

    private fun to(step: DvwStep) = mutableState.update {
        it.copy(
            step = step,
            selected = if (step == DvwStep.Sides) it.fromSide else 0,
            working = false,
            opponentError = null,
        )
    }

    private fun work(action: suspend (DvwState) -> Unit) {
        val s = mutableState.value
        if (s.working) return
        mutableState.value = s.copy(working = true)
        viewModelScope.launch { action(s) }
    }

    /** On to the next step with what's chosen. */
    fun next() = work { s ->
        when (val step = s.step) {
            DvwStep.Sides -> {
                val sides = when (s.selected) {
                    0 -> listOf(DvwSide.HOME)
                    1 -> listOf(DvwSide.VISITING)
                    else -> listOf(DvwSide.HOME, DvwSide.VISITING)
                }
                val days = s.preview?.days.orEmpty()
                mutableState.update { it.copy(sides = sides, current = 0, day = days.singleOrNull()) }
                when (days.size) {
                    1 -> proposeSubstitutions(days[0])
                    else -> to(DvwStep.Day)
                }
            }
            DvwStep.Day -> {
                val days = s.preview?.days.orEmpty()
                val day = if (days.size == 2) days.getOrNull(s.selected) else s.day
                when (day) {
                    null -> mutableState.update { it.copy(working = false) }
                    else -> {
                        mutableState.update { it.copy(day = day) }
                        proposeSubstitutions(day)
                    }
                }
            }
            DvwStep.Substitutions -> when (s.substitutions.toIntOrNull()?.takeIf { it > 0 }) {
                null -> mutableState.update { it.copy(working = false) }
                else -> team(s.copy(current = 0))
            }
            is DvwStep.Team -> when (val team = step.similar.getOrNull(s.selected)) {
                null -> to(DvwStep.NewTeam(step.similar))
                else -> plan(s, team.id)
            }
            is DvwStep.NewTeam -> plan(s, null)
            is DvwStep.Opponent -> {
                val opponent = when (val team = step.similar.getOrNull(s.selected)) {
                    null -> if (s.selected == step.similar.size) DvwOpponent.Name else DvwOpponent.NewTeam
                    else -> DvwOpponent.Team(team.id)
                }
                val choice = step.choice.copy(opponent = opponent)
                try {
                    val ready = app.core { it.planDvw(bytes, choice) }
                    to(DvwStep.Ready(ready, choice))
                } catch (e: MobileException) {
                    mutableState.update { it.copy(opponentError = e.reason, working = false) }
                }
            }
            is DvwStep.Ready -> save(s, step, overwrite = step.ready.savedAlready)
            DvwStep.Done, DvwStep.Loading, is DvwStep.Failed -> mutableState.update { it.copy(working = false) }
        }
    }

    /** The side being imported left as it is: on to the next one. */
    fun skip() = work { s ->
        val team = side(s)
        nextSide(s, DvwResult(team, app.getString(R.string.dvw_skipped, team)))
    }

    /** Back a step: false before the first one. */
    fun back(): Boolean {
        val s = mutableState.value
        if (s.working) return true
        when (val step = s.step) {
            DvwStep.Sides, DvwStep.Loading, is DvwStep.Failed, DvwStep.Done -> return false
            DvwStep.Day -> to(DvwStep.Sides)
            DvwStep.Substitutions -> to(if (s.preview?.days?.size == 1) DvwStep.Sides else DvwStep.Day)
            is DvwStep.Team -> to(DvwStep.Substitutions)
            is DvwStep.NewTeam -> to(if (step.similar.isEmpty()) DvwStep.Substitutions else DvwStep.Team(step.similar))
            is DvwStep.Opponent -> work { team(it) }
            is DvwStep.Ready -> skip()
        }
        return true
    }

    private suspend fun proposeSubstitutions(day: String) {
        val max = app.core { it.dvwMaxSubstitutions(day) }
        mutableState.update { it.copy(substitutions = max.toString()) }
        to(DvwStep.Substitutions)
    }

    private fun side(s: DvwState) = when (s.side) {
        DvwSide.HOME -> s.preview?.home
        DvwSide.VISITING -> s.preview?.visiting
        null -> null
    }.orEmpty()

    /** The team of the side being imported: a similar one or a new one;
     * done after the last side. The team the import was chosen from comes
     * first, until a side goes into it. */
    private suspend fun team(s: DvwState) {
        val side = s.side ?: return to(DvwStep.Done)
        mutableState.update { it.copy(current = s.current, classification = null, gender = null, invalid = null) }
        try {
            val first = file?.teamId?.takeIf { id -> s.results.none { it.teamId == id } }
            val similar = app.core { it.dvwSimilarTeams(bytes, side, first) }
            to(if (similar.isEmpty()) DvwStep.NewTeam(similar) else DvwStep.Team(similar))
        } catch (e: MobileException) {
            nextSide(s, DvwResult(side(s), app.getString(R.string.dvw_not_imported, side(s), e.reason)))
        }
    }

    private fun choice(s: DvwState, teamId: String?) = DvwChoice(
        side = s.side ?: DvwSide.HOME,
        day = s.day.orEmpty(),
        maxSubstitutions = (s.substitutions.toIntOrNull() ?: 0).toUByte(),
        teamId = teamId,
        classification = s.classification,
        gender = s.gender,
        country = s.country?.value,
        opponent = DvwOpponent.Name,
    )

    /** Team A as chosen, checked against B only a name: on to team B, ours
     * like it (team A left out), the team of the side imported before
     * first, else the one the import was chosen from. */
    private suspend fun plan(s: DvwState, teamId: String?) {
        val choice = choice(s, teamId)
        try {
            app.core { it.planDvw(bytes, choice) }
            val first = (s.results.lastOrNull { it.teamId != null }?.teamId ?: file?.teamId)
                ?.takeIf { it != teamId }
            val similar = app.core { it.dvwSimilarOpponents(bytes, choice.side, teamId, first) }
            to(DvwStep.Opponent(similar, choice))
        } catch (e: MobileException.InvalidTeam) {
            // the new team still to fill in, as in the form
            mutableState.update { it.copy(invalid = e.field to e.reason, working = false) }
        } catch (e: MobileException) {
            nextSide(s, DvwResult(side(s), app.getString(R.string.dvw_not_imported, side(s), e.reason)))
        }
    }

    private suspend fun save(s: DvwState, step: DvwStep.Ready, overwrite: Boolean) {
        val team = step.ready.team
        val result = try {
            val imported = app.core { it.importDvw(bytes, step.choice, overwrite) }
            // queued for the statistics: sent now, not at the next start
            app.sendUploads(ExistingWorkPolicy.APPEND_OR_REPLACE)
            DvwResult(team, app.getString(R.string.dvw_imported, team), imported.teamId)
        } catch (e: MobileException) {
            DvwResult(team, app.getString(R.string.dvw_not_imported, team, e.reason))
        }
        nextSide(s, result)
    }

    private suspend fun nextSide(s: DvwState, result: DvwResult) {
        val next = s.copy(current = s.current + 1, results = s.results + result)
        mutableState.update { it.copy(results = next.results, current = next.current) }
        team(next)
    }
}

/** The import of the DataVolley file the app keeps (`file`): none after
 * the process was ended, the screen says so. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun DvwImportScreen(file: DvwOpened?, onBack: () -> Unit, onTeam: (String) -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val model = viewModel { DvwImportViewModel(app, file) }
    val state by model.state.collectAsState()
    BackHandler { if (!model.back()) onBack() }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.import_dvw), Modifier.title()) },
                navigationIcon = { BackButton { if (!model.back()) onBack() } },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        when (val step = state.step) {
            DvwStep.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is DvwStep.Failed -> Centered(modifier) { Text(step.message, Modifier.padding(16.dp)) }
            else -> DvwContent(state, model, onBack, onTeam, modifier)
        }
    }
}

@Composable
private fun DvwContent(
    state: DvwState,
    model: DvwImportViewModel,
    onBack: () -> Unit,
    onTeam: (String) -> Unit,
    modifier: Modifier,
) = Column(modifier) {
    val preview = state.preview ?: return@Column
    val team = when (state.side) {
        DvwSide.HOME -> preview.home
        DvwSide.VISITING -> preview.visiting
        null -> ""
    }
    val opponent = when (state.side) {
        DvwSide.HOME -> preview.visiting
        DvwSide.VISITING -> preview.home
        null -> ""
    }
    Column(
        Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Summary(preview)
        when (val step = state.step) {
            DvwStep.Sides -> {
                Question(stringResource(R.string.dvw_which_team))
                Options(
                    listOf(
                        stringResource(R.string.dvw_home, preview.home),
                        stringResource(R.string.dvw_visiting, preview.visiting),
                        stringResource(R.string.dvw_both),
                    ),
                    state.selected,
                    model::select,
                )
            }
            DvwStep.Day -> when (preview.days.size) {
                2 -> {
                    Question(stringResource(R.string.dvw_which_date))
                    Options(preview.days.map(::day), state.selected, model::select)
                }
                else -> {
                    Question(stringResource(R.string.dvw_no_date))
                    DayField(
                        stringResource(R.string.date),
                        state.day?.let { runCatching { LocalDate.parse(it) }.getOrNull() },
                        onDay = { day -> day?.let { model.day(it.toString()) } },
                    )
                }
            }
            DvwStep.Substitutions -> {
                Question(stringResource(R.string.dvw_substitutions))
                OutlinedTextField(
                    value = state.substitutions,
                    onValueChange = model::substitutions,
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text(stringResource(R.string.max_substitutions)) },
                    singleLine = true,
                    isError = state.substitutions.toIntOrNull()?.takeIf { it > 0 } == null,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                )
            }
            is DvwStep.Team -> {
                Question(stringResource(R.string.dvw_similar_found, team))
                Options(
                    step.similar.map { similar(it) } + stringResource(R.string.dvw_new_team),
                    state.selected,
                    model::select,
                )
            }
            is DvwStep.NewTeam -> {
                Question(stringResource(R.string.dvw_new_team_of, team, preview.season?.toString().orEmpty()))
                NewTeamFields(state, model)
            }
            is DvwStep.Opponent -> {
                Question(stringResource(R.string.dvw_opponent_question, opponent))
                Options(
                    step.similar.map { similar(it) } +
                        stringResource(R.string.dvw_opponent_name, opponent) +
                        stringResource(R.string.dvw_opponent_new, opponent),
                    state.selected,
                    model::select,
                )
                state.opponentError?.let { Text(it, color = MaterialTheme.colorScheme.error) }
            }
            is DvwStep.Ready -> {
                val ready = step.ready
                Question(
                    stringResource(R.string.dvw_ready, ready.team, ready.sets.toInt(), ready.newPlayers.toInt()),
                )
                Text(
                    when (val added = ready.opponentNewPlayers) {
                        null -> stringResource(R.string.dvw_against_name, ready.opponent)
                        else -> stringResource(R.string.dvw_against_team, ready.opponent, added.toInt())
                    },
                )
                if (ready.matchNumbers > 0u) {
                    Text(stringResource(R.string.dvw_match_numbers, ready.matchNumbers.toInt()))
                }
                ready.competition?.let { Text(stringResource(R.string.dvw_competition, it)) }
                if (ready.savedAlready) {
                    Text(stringResource(R.string.dvw_already_saved, ready.team), color = MaterialTheme.colorScheme.error)
                }
            }
            DvwStep.Done -> {
                Question(stringResource(R.string.dvw_done))
                state.results.forEach { result ->
                    when (val id = result.teamId) {
                        null -> Text(result.message)
                        else -> TextButton(onClick = { onTeam(id) }) { Text(result.message) }
                    }
                }
            }
            DvwStep.Loading, is DvwStep.Failed -> {}
        }
    }
    // big and at the bottom, where the thumb is
    val button = Modifier.fillMaxWidth().padding(horizontal = 16.dp).heightIn(min = 56.dp)
    when (val step = state.step) {
        is DvwStep.Ready -> {
            Button(onClick = model::next, enabled = !state.working, modifier = button.padding(top = 16.dp)) {
                Text(stringResource(if (step.ready.savedAlready) R.string.dvw_overwrite else R.string.save))
            }
            OutlinedButton(onClick = { model.skip() }, enabled = !state.working, modifier = button.padding(bottom = 16.dp)) {
                Text(stringResource(R.string.dvw_dont_import))
            }
        }
        DvwStep.Done -> Button(onClick = onBack, modifier = button.padding(vertical = 16.dp)) {
            Text(stringResource(R.string.ok))
        }
        else -> Button(onClick = { model.next() }, enabled = !state.working, modifier = button.padding(vertical = 16.dp)) {
            Text(stringResource(R.string.dvw_next))
        }
    }
}

/** The file: the teams, the day, the season and the sets. */
@Composable
private fun Summary(preview: DvwPreview) = Column {
    Text("${preview.home} - ${preview.visiting}", style = MaterialTheme.typography.titleMedium)
    val days = preview.days.joinToString(" / ", transform = ::day).ifEmpty { "?" }
    Text("$days · ${stringResource(R.string.dvw_season)} ${preview.season ?: "?"}")
    Text("${stringResource(R.string.dvw_sets)}: ${preview.sets.joinToString(", ") { "${it.home}-${it.visiting}" }}")
}

@Composable
private fun Question(text: String) = Text(text, Modifier.title(), style = MaterialTheme.typography.titleSmall)

@Composable
private fun Options(options: List<String>, selected: Int, onSelect: (Int) -> Unit) = Column(Modifier.selectableGroup()) {
    options.forEachIndexed { index, option ->
        Row(
            Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .selectable(selected = index == selected, role = Role.RadioButton, onClick = { onSelect(index) }),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            RadioButton(selected = index == selected, onClick = null)
            Text(option, Modifier.padding(start = 8.dp))
        }
    }
}

/** A similar team, and why. */
@Composable
private fun similar(team: DvwSimilarTeam): String {
    val why = listOfNotNull(
        stringResource(R.string.dvw_same_name).takeIf { team.sameName },
        stringResource(R.string.dvw_same_players, team.samePlayers.toInt(), team.players.toInt())
            .takeIf { team.samePlayers > 0u },
    ).joinToString(", ")
    return if (why.isEmpty()) "${team.name} ${team.year}" else "${team.name} ${team.year} ($why)"
}

/** The gender, the country and the level of the new team, as in the form
 * of a team: the country of the settings to start with. */
@Composable
private fun NewTeamFields(state: DvwState, model: DvwImportViewModel) {
    var choosingCountry by rememberSaveable { mutableStateOf(false) }
    val country = stringResource(R.string.country)
    if (choosingCountry) {
        CountryDialog(
            title = country,
            current = state.country?.value,
            onPick = {
                model.country(it)
                choosingCountry = false
            },
            onDismiss = { choosingCountry = false },
        )
    }
    val error = { field: TeamField -> state.invalid?.takeIf { it.first == field }?.second }
    FieldTitle(stringResource(R.string.gender), error(TeamField.GENDER))
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
        state.genders.forEachIndexed { index, choice ->
            SegmentedButton(
                selected = state.gender == choice.value,
                onClick = { model.gender(choice.value) },
                shape = SegmentedButtonDefaults.itemShape(index, state.genders.size),
            ) { Text(choice.name) }
        }
    }
    CountryField(country, state.country, error(TeamField.COUNTRY)) { choosingCountry = true }
    FieldTitle(stringResource(R.string.classification), error(TeamField.CLASSIFICATION))
    Column(Modifier.selectableGroup()) {
        state.classifications.forEach { choice ->
            ClassificationRow(choice, state.classification == choice.value) { model.classification(choice.value) }
        }
    }
}

/** A day `YYYY-MM-DD` in the language of the phone. */
private fun day(day: String): String = try {
    dayText(LocalDate.parse(day))
} catch (_: java.time.DateTimeException) {
    day
}
