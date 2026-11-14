package io.github.naighes.scout4all

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.ListItem
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
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.naighes.scout4all.core.CompetitionField
import io.github.naighes.scout4all.core.CompetitionForm
import io.github.naighes.scout4all.core.CompetitionListing
import io.github.naighes.scout4all.core.CountryChoice
import io.github.naighes.scout4all.core.FormatChoice
import io.github.naighes.scout4all.core.GenderChoice
import io.github.naighes.scout4all.core.GenderEnum
import io.github.naighes.scout4all.core.MobileException
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

private sealed interface CompetitionsState {
    data object Loading : CompetitionsState
    data class Loaded(val competitions: List<CompetitionListing>) : CompetitionsState
    data class Failed(val message: String) : CompetitionsState
}

/** The competitions, as on the desktop: name, season and format, the
 * matches played in each. A touch edits one, the bin removes it once
 * confirmed (its matches stay, without it); they're created in the form
 * of a match. Read each time it shows. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CompetitionsScreen(onBack: () -> Unit, onCompetition: (String) -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    var removed by rememberSaveable { mutableIntStateOf(0) }
    val state by produceState<CompetitionsState>(CompetitionsState.Loading, removed) {
        value = try {
            CompetitionsState.Loaded(app.core { it.competitionListings() })
        } catch (e: MobileException) {
            CompetitionsState.Failed(e.reason)
        }
    }
    var removing by rememberSaveable { mutableStateOf<String?>(null) }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.competitions), Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val s = state) {
            CompetitionsState.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is CompetitionsState.Failed -> Centered(modifier) { Text(s.message) }
            is CompetitionsState.Loaded -> {
                when {
                    s.competitions.isEmpty() -> Centered(modifier.padding(16.dp)) {
                        Text(stringResource(R.string.no_competitions))
                    }
                    else -> LazyColumn(modifier) {
                        items(s.competitions, key = { it.id }) { c ->
                            CompetitionRow(c, onClick = { onCompetition(c.id) }, onRemove = { removing = c.id })
                        }
                    }
                }
                val competition = s.competitions.firstOrNull { it.id == removing }
                Deletion(
                    competition?.let { c ->
                        when (c.matches) {
                            0u -> stringResource(R.string.delete_competition_unused, c.name)
                            else -> stringResource(R.string.delete_competition_text, c.name, c.matches.toInt())
                        }
                    }.orEmpty(),
                    asking = competition != null,
                    onAsking = { if (!it) removing = null },
                    delete = { core -> competition?.let { core.deleteCompetition(it.id) } },
                    onDeleted = { removed++ },
                )
            }
        }
    }
}

@Composable
private fun CompetitionRow(c: CompetitionListing, onClick: () -> Unit, onRemove: () -> Unit) = ListItem(
    modifier = Modifier.clickable(onClick = onClick),
    headlineContent = { Text(c.name) },
    supportingContent = {
        Column {
            Text(listOfNotNull(c.season, c.format).joinToString(" · "))
            Text(pluralStringResource(R.plurals.matches_count, c.matches.toInt(), c.matches.toInt()))
        }
    },
    trailingContent = {
        IconButton(onClick = onRemove) {
            Icon(Icons.Filled.Delete, contentDescription = "${stringResource(R.string.delete_competition)}: ${c.name}")
        }
    },
)

sealed interface CompetitionFormState {
    data object Loading : CompetitionFormState
    data class Failed(val message: String) : CompetitionFormState
    data class Editing(
        val form: CompetitionForm,
        val formats: List<FormatChoice>,
        val genders: List<GenderChoice>,
        val country: CountryChoice?,
        val invalid: Pair<CompetitionField, String>? = null,
        val error: String? = null,
        val busy: Boolean = false,
        val loaded: CompetitionForm = form,
    ) : CompetitionFormState {
        val changed: Boolean get() = form != loaded
    }
    data object Saved : CompetitionFormState
}

/** A competition being edited: the core checks the form. */
class CompetitionFormViewModel(private val app: Scout4AllApplication, private val id: String) : ViewModel() {
    private val mutableState = MutableStateFlow<CompetitionFormState>(CompetitionFormState.Loading)
    val state: StateFlow<CompetitionFormState> = mutableState

    init {
        viewModelScope.launch {
            mutableState.value = try {
                app.core { core ->
                    val form = core.competitionForm(id)
                    CompetitionFormState.Editing(
                        form,
                        core.competitionFormats(),
                        core.genders(),
                        form.country?.let { CountryChoice(it, core.countryName(it)) },
                    )
                }
            } catch (e: MobileException) {
                CompetitionFormState.Failed(e.reason)
            }
        }
    }

    private fun update(edit: (CompetitionFormState.Editing) -> CompetitionFormState.Editing) = mutableState.update { s ->
        when (s) {
            is CompetitionFormState.Editing -> edit(s)
            else -> s
        }
    }

    fun change(field: CompetitionField?, edit: (CompetitionForm) -> CompetitionForm) = update { s ->
        s.copy(form = edit(s.form), invalid = s.invalid?.takeIf { it.first != field }, error = null)
    }

    fun pickCountry(choice: CountryChoice?) {
        change(CompetitionField.COUNTRY) { it.copy(country = choice?.value) }
        update { it.copy(country = choice) }
    }

    fun save() {
        val editing = mutableState.value as? CompetitionFormState.Editing ?: return
        if (editing.busy) return
        mutableState.value = editing.copy(busy = true)
        viewModelScope.launch {
            mutableState.value = try {
                app.core { it.saveCompetition(id, editing.form) }
                CompetitionFormState.Saved
            } catch (e: MobileException.InvalidCompetition) {
                editing.copy(invalid = e.field to e.reason, error = null)
            } catch (e: MobileException) {
                editing.copy(error = e.reason, invalid = null)
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CompetitionFormScreen(id: String, onBack: () -> Unit, onSaved: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val model = viewModel { CompetitionFormViewModel(app, id) }
    val state by model.state.collectAsState()
    val back = rememberFormBack((state as? CompetitionFormState.Editing)?.changed == true, onBack)
    LaunchedEffect(state) {
        if (state == CompetitionFormState.Saved) onSaved()
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.edit_competition), Modifier.title()) },
                navigationIcon = { BackButton(back) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        when (val s = state) {
            CompetitionFormState.Loading, CompetitionFormState.Saved -> Centered(modifier) { CircularProgressIndicator() }
            is CompetitionFormState.Failed -> Centered(modifier) { Text(s.message) }
            is CompetitionFormState.Editing -> CompetitionFormContent(s, model, modifier)
        }
    }
}

/** Name, season, format, gender and country (both may be none), and
 * whether its new matches are friendly by default. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CompetitionFormContent(state: CompetitionFormState.Editing, model: CompetitionFormViewModel, modifier: Modifier) =
    Column(modifier) {
        val form = state.form
        val error = { field: CompetitionField -> state.invalid?.takeIf { it.first == field }?.second }
        var choosingCountry by rememberSaveable { mutableStateOf(false) }
        val countryLabel = stringResource(R.string.country)
        if (choosingCountry) {
            CountryDialog(
                title = countryLabel,
                current = form.country,
                onPick = {
                    model.pickCountry(it)
                    choosingCountry = false
                },
                onDismiss = { choosingCountry = false },
            )
        }
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            OutlinedTextField(
                value = form.name,
                onValueChange = { name -> model.change(CompetitionField.NAME) { it.copy(name = name) } },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.name)) },
                singleLine = true,
                isError = error(CompetitionField.NAME) != null,
                supportingText = error(CompetitionField.NAME)?.let { { Text(it) } },
                keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words, imeAction = ImeAction.Next),
            )
            OutlinedTextField(
                value = form.season,
                onValueChange = { season -> model.change(CompetitionField.NAME) { it.copy(season = season) } },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.season)) },
                placeholder = { Text(stringResource(R.string.season_placeholder)) },
                singleLine = true,
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Done),
            )
            ChoiceField(
                stringResource(R.string.competition_format),
                state.formats.map { it.value to it.name },
                form.format,
                onSelect = { format -> model.change(null) { it.copy(format = format) } },
            )
            ChoiceField(
                stringResource(R.string.gender),
                listOf<Pair<GenderEnum?, String>>(null to NO_CHOICE) + state.genders.map { it.value to it.name },
                form.gender,
                onSelect = { gender -> model.change(null) { it.copy(gender = gender) } },
            )
            CountryField(countryLabel, state.country, error(CompetitionField.COUNTRY)) { choosingCountry = true }
            if (form.country != null) {
                TextButton(onClick = { model.pickCountry(null) }) { Text(stringResource(R.string.none)) }
            }
            CheckRow(stringResource(R.string.friendly_default), form.friendly, onChecked = { friendly ->
                model.change(null) { it.copy(friendly = friendly) }
            })
        }
        state.error?.let { Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error) }
        Button(
            onClick = model::save,
            enabled = !state.busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
        ) { Text(stringResource(R.string.save)) }
    }
