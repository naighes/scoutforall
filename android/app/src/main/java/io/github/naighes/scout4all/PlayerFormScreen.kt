package io.github.naighes.scout4all

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.PlayerField
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleChoice
import io.github.naighes.scout4all.core.Scout4All
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

sealed interface PlayerFormState {
    data object Loading : PlayerFormState
    data class Failed(val message: String) : PlayerFormState
    data class Editing(
        val form: PlayerForm,
        val roles: List<RoleChoice>,
        /** the name as saved, for the confirmation of the removal */
        val savedName: String = "",
        /** the field the core rejected, with why */
        val invalid: Pair<PlayerField, String>? = null,
        /** an error of the save or of the removal */
        val error: String? = null,
        val busy: Boolean = false,
        val confirmingRemoval: Boolean = false,
        /** the form as it was read: changed, going back asks first */
        val loaded: PlayerForm = form,
    ) : PlayerFormState {
        val changed: Boolean get() = form != loaded
    }
    /** saved or removed: the screen goes back */
    data object Done : PlayerFormState
}

/** A new player of the team (`playerId` null) or one being edited. The core
 * checks the form. */
class PlayerFormViewModel(
    private val app: Scout4AllApplication,
    private val teamId: String,
    private val playerId: String?,
) : ViewModel() {
    private val mutableState = MutableStateFlow<PlayerFormState>(PlayerFormState.Loading)
    val state: StateFlow<PlayerFormState> = mutableState

    init {
        viewModelScope.launch {
            mutableState.value = try {
                app.core { core ->
                    val player = playerId?.let { id -> core.team(teamId).players.first { it.id == id } }
                    PlayerFormState.Editing(
                        form = player?.form ?: PlayerForm(name = "", role = null, number = ""),
                        roles = core.roles(),
                        savedName = player?.name.orEmpty(),
                    )
                }
            } catch (e: MobileException) {
                PlayerFormState.Failed(e.reason)
            } catch (e: NoSuchElementException) {
                PlayerFormState.Failed(e.message.orEmpty())
            }
        }
    }

    private fun update(edit: (PlayerFormState.Editing) -> PlayerFormState.Editing) = mutableState.update { s ->
        when (s) {
            is PlayerFormState.Editing -> edit(s)
            else -> s
        }
    }

    /** A change of the form: the error of the field changed goes away. */
    fun change(field: PlayerField, edit: (PlayerForm) -> PlayerForm) = update { s ->
        s.copy(form = edit(s.form), invalid = s.invalid?.takeIf { it.first != field }, error = null)
    }

    fun confirmRemoval(confirming: Boolean) = update { it.copy(confirmingRemoval = confirming) }

    fun save() = submit { core ->
        core.savePlayer(teamId, playerId, (mutableState.value as PlayerFormState.Editing).form)
    }

    fun remove() = playerId?.let { id -> submit { core -> core.removePlayer(teamId, id) } }

    /** Saves or removes, once at a time. */
    private fun submit(call: (Scout4All) -> Unit) {
        val editing = mutableState.value as? PlayerFormState.Editing ?: return
        if (editing.busy) return
        mutableState.value = editing.copy(busy = true, confirmingRemoval = false)
        viewModelScope.launch {
            mutableState.value = try {
                app.core(call)
                PlayerFormState.Done
            } catch (e: MobileException.InvalidPlayer) {
                editing.copy(invalid = e.field to e.reason, error = null, confirmingRemoval = false)
            } catch (e: MobileException) {
                editing.copy(error = e.reason, invalid = null, confirmingRemoval = false)
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PlayerFormScreen(teamId: String, playerId: String?, onBack: () -> Unit, onDone: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val model = viewModel { PlayerFormViewModel(app, teamId, playerId) }
    val state by model.state.collectAsState()
    val back = rememberFormBack((state as? PlayerFormState.Editing)?.changed == true, onBack)
    LaunchedEffect(state) {
        if (state == PlayerFormState.Done) onDone()
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(if (playerId == null) R.string.new_player else R.string.edit_player)) },
                navigationIcon = { BackButton(back) },
                actions = {
                    if (playerId != null && state is PlayerFormState.Editing) {
                        IconButton(onClick = { model.confirmRemoval(true) }) {
                            Icon(Icons.Filled.Delete, contentDescription = stringResource(R.string.remove_player))
                        }
                    }
                },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        when (val s = state) {
            PlayerFormState.Loading, PlayerFormState.Done -> Centered(modifier) { CircularProgressIndicator() }
            is PlayerFormState.Failed -> Centered(modifier) { Text(s.message) }
            is PlayerFormState.Editing -> {
                PlayerFormContent(s, model, modifier)
                if (s.confirmingRemoval) RemovalDialog(s.savedName, model)
            }
        }
    }
}

@Composable
private fun PlayerFormContent(state: PlayerFormState.Editing, model: PlayerFormViewModel, modifier: Modifier) =
    Column(modifier) {
        val form = state.form
        val error = { field: PlayerField -> state.invalid?.takeIf { it.first == field }?.second }
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            OutlinedTextField(
                value = form.name,
                onValueChange = { name -> model.change(PlayerField.NAME) { it.copy(name = name) } },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.name)) },
                singleLine = true,
                isError = error(PlayerField.NAME) != null,
                supportingText = error(PlayerField.NAME)?.let { { Text(it) } },
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.Words,
                    imeAction = ImeAction.Next,
                ),
            )
            OutlinedTextField(
                value = form.number,
                onValueChange = { number -> model.change(PlayerField.NUMBER) { it.copy(number = number) } },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.number)) },
                singleLine = true,
                isError = error(PlayerField.NUMBER) != null,
                supportingText = error(PlayerField.NUMBER)?.let { { Text(it) } },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
            )
            FieldTitle(stringResource(R.string.role), error(PlayerField.ROLE))
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                state.roles.forEach { choice ->
                    FilterChip(
                        selected = form.role == choice.value,
                        onClick = { model.change(PlayerField.ROLE) { it.copy(role = choice.value) } },
                        label = { Text(choice.name) },
                        modifier = Modifier.heightIn(min = 48.dp),
                    )
                }
            }
        }
        state.error?.let {
            Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
        }
        // big and at the bottom, where the thumb is
        Button(
            onClick = model::save,
            enabled = !state.busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
        ) { Text(stringResource(R.string.save)) }
    }

@Composable
private fun RemovalDialog(name: String, model: PlayerFormViewModel) = AlertDialog(
    onDismissRequest = { model.confirmRemoval(false) },
    title = { Text(stringResource(R.string.remove_player_title, name)) },
    text = { Text(stringResource(R.string.remove_player_text)) },
    confirmButton = {
        TextButton(onClick = { model.remove() }) {
            Text(stringResource(R.string.remove), color = MaterialTheme.colorScheme.error)
        }
    },
    dismissButton = {
        TextButton(onClick = { model.confirmRemoval(false) }) { Text(stringResource(R.string.cancel)) }
    },
)
