package io.github.naighes.scout4all

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.TextButton
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.text.style.TextAlign
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
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
import io.github.naighes.scout4all.core.CountryChoice
import io.github.naighes.scout4all.core.GenderChoice
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.TeamField
import io.github.naighes.scout4all.core.TeamForm
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
        /** the country of the form, with its name */
        val country: CountryChoice?,
        /** the field the core rejected, with why */
        val invalid: Pair<TeamField, String>? = null,
        /** an error of the save itself */
        val error: String? = null,
        val saving: Boolean = false,
        /** the form as it was read: changed, going back asks first */
        val loaded: TeamForm = form,
        /** the logo the team has now, as PNG */
        val stored: ByteArray? = null,
        /** what the form does with it once saved */
        val logo: LogoEdit = LogoEdit.Keep,
    ) : TeamFormState {
        val changed: Boolean get() = form != loaded || logo != LogoEdit.Keep
        /** the logo the preview shows */
        val shown: ByteArray? get() = when (logo) {
            LogoEdit.Keep -> stored
            is LogoEdit.Replace -> logo.bytes
            LogoEdit.Remove -> null
        }
    }
    /** saved: the screen goes back */
    data object Saved : TeamFormState
}

/** What the form does with the logo of the team when saved. */
sealed interface LogoEdit {
    data object Keep : LogoEdit
    data object Remove : LogoEdit
    class Replace(val bytes: ByteArray) : LogoEdit
}

/** A new team (`id` null) or one being edited. The core checks the form. */
class TeamFormViewModel(private val app: Scout4AllApplication, private val id: String?) : ViewModel() {
    private val mutableState = MutableStateFlow<TeamFormState>(TeamFormState.Loading)
    val state: StateFlow<TeamFormState> = mutableState

    init {
        viewModelScope.launch {
            mutableState.value = try {
                app.core { core ->
                    val form = id?.let { core.team(it).form }
                        ?: core.newTeamForm(app.clock.today().year.toUShort())
                    TeamFormState.Editing(
                        form = form,
                        classifications = core.classifications(),
                        genders = core.genders(),
                        country = form.country?.let { CountryChoice(it, core.countryName(it)) },
                        stored = id?.let { core.teamLogo(it) },
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

    /** The dialog of what went wrong closed. */
    fun dismissError() = mutableState.update { s -> (s as? TeamFormState.Editing)?.copy(error = null) ?: s }

    /** The country chosen in the list. */
    fun pickCountry(choice: CountryChoice) {
        change(TeamField.COUNTRY) { it.copy(country = choice.value) }
        mutableState.update { s -> (s as? TeamFormState.Editing)?.copy(country = choice) ?: s }
    }

    /** The image chosen: it has to be one, the core shrinks it on save. */
    fun pickLogo(bytes: ByteArray) = mutableState.update { s ->
        when {
            s !is TeamFormState.Editing -> s
            decodeImage(bytes) == null -> s.copy(error = app.getString(R.string.file_not_read))
            else -> s.copy(logo = LogoEdit.Replace(bytes), error = null)
        }
    }

    fun removeLogo() = mutableState.update { s ->
        (s as? TeamFormState.Editing)?.copy(logo = LogoEdit.Remove, error = null) ?: s
    }

    fun save() {
        val editing = mutableState.value as? TeamFormState.Editing ?: return
        if (editing.saving) return
        mutableState.value = editing.copy(saving = true)
        viewModelScope.launch {
            mutableState.value = try {
                app.core { core ->
                    val saved = core.saveTeam(id, editing.form)
                    when (val logo = editing.logo) {
                        LogoEdit.Keep -> {}
                        LogoEdit.Remove -> core.setTeamLogo(saved, null)
                        is LogoEdit.Replace -> core.setTeamLogo(saved, logo.bytes)
                    }
                }
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
    val back = rememberFormBack((state as? TeamFormState.Editing)?.changed == true, onBack)
    LaunchedEffect(state) {
        if (state == TeamFormState.Saved) onSaved()
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(if (id == null) R.string.new_team else R.string.edit_team), Modifier.title()) },
                navigationIcon = { BackButton(back) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        when (val s = state) {
            TeamFormState.Loading, TeamFormState.Saved -> Centered(modifier) { CircularProgressIndicator() }
            is TeamFormState.Failed -> LoadFailed(s.message, modifier)
            is TeamFormState.Editing -> TeamFormContent(s, model, modifier)
        }
    }
}

@Composable
private fun TeamFormContent(state: TeamFormState.Editing, model: TeamFormViewModel, modifier: Modifier) =
    Column(modifier) {
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            LogoPanel(state.shown, model::pickLogo, model::removeLogo)
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
        state.error?.let { NoticeDialog(Notice.error(it), model::dismissError) }
        // big and at the bottom, where the thumb is
        Button(
            onClick = model::save,
            enabled = !state.saving,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
        ) { Text(stringResource(R.string.save)) }
    }

/** The logo with its preview, the buttons to choose (gallery, files) or
 * remove it. */
@Composable
private fun LogoPanel(shown: ByteArray?, onPick: (ByteArray) -> Unit, onRemove: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        uri ?: return@rememberLauncherForActivityResult
        // not read: the picture is not an image either
        scope.launch { onPick(readUri(context, uri)) }
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
        Box(Modifier.size(96.dp), contentAlignment = Alignment.Center) {
            if (shown == null) {
                Text(stringResource(R.string.no_logo), style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center)
            } else {
                TeamLogoBox(shown, 96.dp)
            }
        }
        Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.logo), Modifier.title(), style = MaterialTheme.typography.titleSmall)
            OutlinedButton(onClick = { pick.launch("image/*") }) { Text(stringResource(R.string.choose_logo)) }
            if (shown != null) {
                TextButton(onClick = onRemove) { Text(stringResource(R.string.remove_logo)) }
            }
        }
    }
}

/** The fields of a team (of its form, of a new season), `error` the one
 * of each as the core tells. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TeamFields(
    form: TeamForm,
    genders: List<GenderChoice>,
    classifications: List<ClassificationChoice>,
    country: CountryChoice?,
    error: (TeamField) -> String?,
    onChange: (TeamField, (TeamForm) -> TeamForm) -> Unit,
    onPickCountry: (CountryChoice) -> Unit,
) {
    var choosingCountry by rememberSaveable { mutableStateOf(false) }
    val countryLabel = stringResource(R.string.country)
    if (choosingCountry) {
        CountryDialog(
            title = countryLabel,
            current = form.country,
            onPick = {
                onPickCountry(it)
                choosingCountry = false
            },
            onDismiss = { choosingCountry = false },
        )
    }
    OutlinedTextField(
        value = form.name,
        onValueChange = { name -> onChange(TeamField.NAME) { it.copy(name = name) } },
        modifier = Modifier.fillMaxWidth(),
        label = { Text(stringResource(R.string.name)) },
        singleLine = true,
        isError = error(TeamField.NAME) != null,
        supportingText = noteOf(error(TeamField.NAME)),
        keyboardOptions = KeyboardOptions(
            capitalization = KeyboardCapitalization.Words,
            imeAction = ImeAction.Next,
        ),
    )
    OutlinedTextField(
        value = form.year,
        onValueChange = { year -> onChange(TeamField.YEAR) { it.copy(year = year) } },
        modifier = Modifier.fillMaxWidth(),
        label = { Text(stringResource(R.string.year)) },
        singleLine = true,
        isError = error(TeamField.YEAR) != null,
        supportingText = noteOf(error(TeamField.YEAR)),
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
    )
    FieldTitle(stringResource(R.string.gender), error(TeamField.GENDER))
    SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
        genders.forEachIndexed { index, choice ->
            SegmentedButton(
                selected = form.gender == choice.value,
                onClick = { onChange(TeamField.GENDER) { it.copy(gender = choice.value) } },
                shape = SegmentedButtonDefaults.itemShape(index, genders.size),
            ) { Text(choice.name) }
        }
    }
    CountryField(countryLabel, country, error(TeamField.COUNTRY)) { choosingCountry = true }
    FieldTitle(stringResource(R.string.classification), error(TeamField.CLASSIFICATION))
    Column(Modifier.selectableGroup()) {
        classifications.forEach { choice ->
            ClassificationRow(choice, form.classification == choice.value) {
                onChange(TeamField.CLASSIFICATION) { it.copy(classification = choice.value) }
            }
        }
    }
}

@Composable
fun FieldTitle(title: String, error: String?) = Column {
    Text(title, Modifier.title(), style = MaterialTheme.typography.titleSmall)
    error?.let { FieldNote(it, Modifier.padding(top = 4.dp)) }
}

@Composable
fun ClassificationRow(choice: ClassificationChoice, selected: Boolean, onSelect: () -> Unit) = Row(
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
