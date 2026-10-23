package io.github.naighes.scout4all

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
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.naighes.scout4all.core.ClassificationChoice
import io.github.naighes.scout4all.core.GenderChoice
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.TeamField
import io.github.naighes.scout4all.core.TeamForm
import java.time.Year
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

sealed interface TeamFormState {
    data object Loading : TeamFormState
    data class Failed(val message: String) : TeamFormState
    data class Editing(
        val form: TeamForm,
        val classifications: List<ClassificationChoice>,
        val genders: List<GenderChoice>,
        /** the field the core rejected, with why */
        val invalid: Pair<TeamField, String>? = null,
        /** an error of the save itself */
        val error: String? = null,
        val saving: Boolean = false,
    ) : TeamFormState
    /** saved: the screen goes back */
    data object Saved : TeamFormState
}

/** A new team (`id` null) or one being edited. The core checks the form. */
class TeamFormViewModel(private val app: Scout4AllApplication, private val id: String?) : ViewModel() {
    private val mutableState = MutableStateFlow<TeamFormState>(TeamFormState.Loading)
    val state: StateFlow<TeamFormState> = mutableState

    init {
        viewModelScope.launch {
            mutableState.value = try {
                app.core { core ->
                    TeamFormState.Editing(
                        form = id?.let { core.team(it).form }
                            ?: core.newTeamForm(Year.now().value.toUShort()),
                        classifications = core.classifications(),
                        genders = core.genders(),
                    )
                }
            } catch (e: MobileException) {
                TeamFormState.Failed(e.reason)
            }
        }
    }

    /** A change of the form: the error of the field changed goes away. */
    fun change(field: TeamField, edit: (TeamForm) -> TeamForm) = mutableState.update { s ->
        when (s) {
            is TeamFormState.Editing -> s.copy(
                form = edit(s.form),
                invalid = s.invalid?.takeIf { it.first != field },
                error = null,
            )
            else -> s
        }
    }

    fun save() {
        val editing = mutableState.value as? TeamFormState.Editing ?: return
        if (editing.saving) return
        mutableState.value = editing.copy(saving = true)
        viewModelScope.launch {
            mutableState.value = try {
                app.core { it.saveTeam(id, editing.form) }
                TeamFormState.Saved
            } catch (e: MobileException.InvalidTeam) {
                editing.copy(invalid = e.field to e.reason, error = null)
            } catch (e: MobileException) {
                editing.copy(error = e.reason, invalid = null)
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TeamFormScreen(id: String?, onBack: () -> Unit, onSaved: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val model = viewModel { TeamFormViewModel(app, id) }
    val state by model.state.collectAsState()
    LaunchedEffect(state) {
        if (state == TeamFormState.Saved) onSaved()
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(if (id == null) R.string.new_team else R.string.edit_team)) },
                navigationIcon = { BackButton(onBack) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        when (val s = state) {
            TeamFormState.Loading, TeamFormState.Saved -> Centered(modifier) { CircularProgressIndicator() }
            is TeamFormState.Failed -> Centered(modifier) { Text(s.message) }
            is TeamFormState.Editing -> TeamFormContent(s, model, modifier)
        }
    }
}

@Composable
private fun TeamFormContent(state: TeamFormState.Editing, model: TeamFormViewModel, modifier: Modifier) =
    Column(modifier) {
        val form = state.form
        val error = { field: TeamField -> state.invalid?.takeIf { it.first == field }?.second }
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            OutlinedTextField(
                value = form.name,
                onValueChange = { name -> model.change(TeamField.NAME) { it.copy(name = name) } },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.name)) },
                singleLine = true,
                isError = error(TeamField.NAME) != null,
                supportingText = error(TeamField.NAME)?.let { { Text(it) } },
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.Words,
                    imeAction = ImeAction.Next,
                ),
            )
            OutlinedTextField(
                value = form.year,
                onValueChange = { year -> model.change(TeamField.YEAR) { it.copy(year = year) } },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.year)) },
                singleLine = true,
                isError = error(TeamField.YEAR) != null,
                supportingText = error(TeamField.YEAR)?.let { { Text(it) } },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
            )
            FieldTitle(stringResource(R.string.gender), error(TeamField.GENDER))
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                state.genders.forEachIndexed { index, choice ->
                    SegmentedButton(
                        selected = form.gender == choice.value,
                        onClick = { model.change(TeamField.GENDER) { it.copy(gender = choice.value) } },
                        shape = SegmentedButtonDefaults.itemShape(index, state.genders.size),
                    ) { Text(choice.name) }
                }
            }
            FieldTitle(stringResource(R.string.classification), error(TeamField.CLASSIFICATION))
            Column(Modifier.selectableGroup()) {
                state.classifications.forEach { choice ->
                    ClassificationRow(choice, form.classification == choice.value) {
                        model.change(TeamField.CLASSIFICATION) { it.copy(classification = choice.value) }
                    }
                }
            }
        }
        state.error?.let {
            Text(
                it,
                Modifier.padding(horizontal = 16.dp),
                color = MaterialTheme.colorScheme.error,
            )
        }
        // big and at the bottom, where the thumb is
        Button(
            onClick = model::save,
            enabled = !state.saving,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
        ) { Text(stringResource(R.string.save)) }
    }

@Composable
fun FieldTitle(title: String, error: String?) = Column {
    Text(title, style = MaterialTheme.typography.titleSmall)
    error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
}

@Composable
private fun ClassificationRow(choice: ClassificationChoice, selected: Boolean, onSelect: () -> Unit) = Row(
    Modifier
        .fillMaxWidth()
        .selectable(selected = selected, onClick = onSelect, role = Role.RadioButton)
        .padding(vertical = 8.dp),
    verticalAlignment = Alignment.Top,
) {
    RadioButton(selected = selected, onClick = null, modifier = Modifier.padding(end = 12.dp, top = 2.dp))
    Column {
        Text(choice.name, style = MaterialTheme.typography.bodyLarge)
        Text(
            choice.description,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}
