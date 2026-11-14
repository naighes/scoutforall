package io.github.naighes.scout4all

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.grid.GridCells
import androidx.compose.foundation.lazy.grid.LazyVerticalGrid
import androidx.compose.foundation.lazy.grid.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.naighes.scout4all.core.ClassificationChoice
import io.github.naighes.scout4all.core.CountryChoice
import io.github.naighes.scout4all.core.GenderChoice
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.RoleChoice
import io.github.naighes.scout4all.core.RoleEnum
import io.github.naighes.scout4all.core.SeasonPlayer
import io.github.naighes.scout4all.core.TeamField
import io.github.naighes.scout4all.core.TeamForm
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

sealed interface SeasonState {
    data object Loading : SeasonState
    data class Failed(val message: String) : SeasonState
    data class Editing(
        /** the new team: as the old one, the year after */
        val form: TeamForm,
        /** the active players of the team, all chosen, by number */
        val players: List<SeasonPlayer>,
        val genders: List<GenderChoice>,
        val classifications: List<ClassificationChoice>,
        val country: CountryChoice?,
        val roles: List<RoleChoice>,
        /** the field of the team the core rejected, with why */
        val invalid: Pair<TeamField, String>? = null,
        val error: String? = null,
        val busy: Boolean = false,
        /** the second step: who goes on to the new season */
        val onPlayers: Boolean = false,
        val correcting: SeasonCorrection? = null,
        val loaded: Pair<TeamForm, List<SeasonPlayer>> = form to players,
    ) : SeasonState {
        val changed: Boolean get() = (form to players) != loaded
    }
    /** saved, with the id of the new team */
    data class Saved(val teamId: String) : SeasonState
}

/** The number and the role of a player of the new season being corrected,
 * as written; why not, when the core says so. */
data class SeasonCorrection(
    val player: SeasonPlayer,
    val number: String,
    val role: RoleEnum?,
    val error: String? = null,
)

/** The new season of a team (D15): its form, then the players taken
 * there, the same persons with number and role to correct. The old team
 * stays. */
class NewSeasonViewModel(private val app: Scout4AllApplication, private val teamId: String) : ViewModel() {
    private val mutableState = MutableStateFlow<SeasonState>(SeasonState.Loading)
    val state: StateFlow<SeasonState> = mutableState

    init {
        viewModelScope.launch {
            mutableState.value = try {
                app.core { core ->
                    val season = core.newSeason(teamId)
                    SeasonState.Editing(
                        season.form,
                        season.players,
                        core.genders(),
                        core.classifications(),
                        season.form.country?.let { CountryChoice(it, core.countryName(it)) },
                        core.roles(teamId),
                    )
                }
            } catch (e: MobileException) {
                SeasonState.Failed(e.reason)
            }
        }
    }

    private fun update(edit: (SeasonState.Editing) -> SeasonState.Editing) = mutableState.update { s ->
        when (s) {
            is SeasonState.Editing -> edit(s)
            else -> s
        }
    }

    fun change(field: TeamField, edit: (TeamForm) -> TeamForm) = update { s ->
        s.copy(form = edit(s.form), invalid = s.invalid?.takeIf { it.first != field }, error = null)
    }

    fun pickCountry(choice: CountryChoice) {
        change(TeamField.COUNTRY) { it.copy(country = choice.value) }
        update { it.copy(country = choice) }
    }

    fun next() = update { it.copy(onPlayers = true, error = null) }

    fun toFields() = update { it.copy(onPlayers = false, error = null) }

    /** The player taken to the new season, or left. */
    fun toggle(player: String) = update { s ->
        s.copy(players = s.players.map { if (it.id == player) it.copy(chosen = !it.chosen) else it }, error = null)
    }

    fun correcting(player: SeasonPlayer?) = update { s ->
        s.copy(correcting = player?.let { SeasonCorrection(it, it.number.toString(), it.role) })
    }

    fun editCorrection(edit: (SeasonCorrection) -> SeasonCorrection) = update { s ->
        s.copy(correcting = s.correcting?.let { edit(it).copy(error = null) })
    }

    private val correctingNow = Mutex()

    /** The number and the role as corrected, checked by the core: the
     * players by number again, or the dialog with why. */
    fun correct() {
        viewModelScope.launch {
            correctingNow.withLock {
                val editing = mutableState.value as? SeasonState.Editing ?: return@withLock
                val c = editing.correcting ?: return@withLock
                try {
                    val players = app.core {
                        it.correctSeasonPlayer(teamId, editing.players, c.player.id, c.number.trim(), c.role)
                    }
                    update { it.copy(players = players, correcting = null, error = null) }
                } catch (e: MobileException) {
                    update { s -> s.copy(correcting = s.correcting?.copy(error = e.reason)) }
                }
            }
        }
    }

    /** The new team saved with the players chosen; an error of the team
     * back on its fields. */
    fun save() {
        val editing = mutableState.value as? SeasonState.Editing ?: return
        if (editing.busy) return
        mutableState.value = editing.copy(busy = true)
        viewModelScope.launch {
            mutableState.value = try {
                SeasonState.Saved(app.core { it.saveNewSeason(teamId, editing.form, editing.players) })
            } catch (e: MobileException.InvalidTeam) {
                editing.copy(invalid = e.field to e.reason, error = null, onPlayers = false)
            } catch (e: MobileException) {
                editing.copy(error = e.reason, invalid = null)
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewSeasonScreen(teamId: String, onBack: () -> Unit, onSaved: (String) -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val model = viewModel { NewSeasonViewModel(app, teamId) }
    val state by model.state.collectAsState()
    val back = rememberFormBack((state as? SeasonState.Editing)?.changed == true, onBack)
    val onPlayers = (state as? SeasonState.Editing)?.onPlayers == true
    BackHandler(enabled = onPlayers) { model.toFields() }
    LaunchedEffect(state) {
        (state as? SeasonState.Saved)?.let { onSaved(it.teamId) }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Text(stringResource(if (onPlayers) R.string.season_players else R.string.new_season), Modifier.title())
                },
                navigationIcon = { BackButton(if (onPlayers) model::toFields else back) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        when (val s = state) {
            SeasonState.Loading, is SeasonState.Saved -> Centered(modifier) { CircularProgressIndicator() }
            is SeasonState.Failed -> Centered(modifier) { Text(s.message) }
            is SeasonState.Editing -> {
                when (s.onPlayers) {
                    true -> SeasonPlayers(s, model, modifier)
                    false -> SeasonFields(s, model, modifier)
                }
                s.correcting?.let { c ->
                    NumberRoleDialog(
                        "${c.player.number} ${c.player.name}",
                        c.number,
                        c.role,
                        s.roles,
                        c.error,
                        onNumber = { number -> model.editCorrection { it.copy(number = number) } },
                        onRole = { role -> model.editCorrection { it.copy(role = role) } },
                        onOk = model::correct,
                        onCancel = { model.correcting(null) },
                    )
                }
            }
        }
    }
}

@Composable
private fun SeasonFields(state: SeasonState.Editing, model: NewSeasonViewModel, modifier: Modifier) = Column(modifier) {
    Column(
        Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        TeamFields(
            state.form,
            state.genders,
            state.classifications,
            state.country,
            error = { field -> state.invalid?.takeIf { it.first == field }?.second },
            onChange = model::change,
            onPickCountry = model::pickCountry,
        )
    }
    state.error?.let { Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error) }
    Button(
        onClick = model::next,
        modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
    ) { Text(stringResource(R.string.go_on)) }
}

/** The players as tiles, the chosen ones lit: a tap takes one or leaves
 * them, the pencil corrects number and role. */
@Composable
private fun SeasonPlayers(state: SeasonState.Editing, model: NewSeasonViewModel, modifier: Modifier) = Column(modifier) {
    Text(
        stringResource(R.string.season_count, state.players.count { it.chosen }),
        Modifier.padding(horizontal = 16.dp, vertical = 8.dp).title(),
        style = MaterialTheme.typography.titleMedium,
    )
    LazyVerticalGrid(
        columns = GridCells.Adaptive(minSize = 104.dp * LocalDensity.current.fontScale.coerceAtLeast(1f)),
        modifier = Modifier.weight(1f),
        contentPadding = PaddingValues(16.dp),
        horizontalArrangement = Arrangement.spacedBy(12.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        items(state.players, key = { it.id }) { player ->
            PlayerTile(
                player.number.toInt(),
                player.name,
                listOfNotNull(player.roleName),
                chosen = player.chosen,
                locked = false,
                onCorrect = { model.correcting(player) },
            ) { model.toggle(player.id) }
        }
    }
    state.error?.let { Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error) }
    Button(
        onClick = model::save,
        enabled = !state.busy,
        modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
    ) { Text(stringResource(R.string.save)) }
}
