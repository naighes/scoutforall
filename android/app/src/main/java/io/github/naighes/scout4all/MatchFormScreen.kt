package io.github.naighes.scout4all

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
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.naighes.scout4all.core.MatchField
import io.github.naighes.scout4all.core.MatchForm
import io.github.naighes.scout4all.core.MobileException
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

sealed interface MatchFormState {
    data object Loading : MatchFormState
    data class Failed(val message: String) : MatchFormState
    data class Editing(
        val form: MatchForm,
        /** the field the core rejected, with why */
        val invalid: Pair<MatchField, String>? = null,
        /** an error of the save */
        val error: String? = null,
        val busy: Boolean = false,
        /** the form as it was read: changed, going back asks first */
        val loaded: MatchForm = form,
    ) : MatchFormState {
        val changed: Boolean get() = form != loaded
    }
    /** saved, with the id of the match: the screen goes on */
    data class Saved(val matchId: String) : MatchFormState
}

/** A new match of the team (`matchId` null), today, or one being edited.
 * The core checks the form. The `example` match is the one of the
 * tutorial, ready to save. */
class MatchFormViewModel(
    private val app: Scout4AllApplication,
    private val teamId: String,
    private val matchId: String?,
    example: Boolean = false,
) : ViewModel() {
    private val mutableState = MutableStateFlow<MatchFormState>(MatchFormState.Loading)
    val state: StateFlow<MatchFormState> = mutableState

    init {
        viewModelScope.launch {
            mutableState.value = try {
                app.core { core ->
                    MatchFormState.Editing(
                        when (matchId) {
                            null -> when (example) {
                                true -> core.tutorialMatchForm(teamId, rfc3339(OffsetDateTime.now()))
                                false -> core.newMatchForm(rfc3339(OffsetDateTime.now()))
                            }
                            else -> core.matchForm(teamId, matchId)
                        },
                    )
                }
            } catch (e: MobileException) {
                MatchFormState.Failed(e.reason)
            }
        }
    }

    /** A change of the form: the error of the field changed, if any, goes
     * away. */
    fun change(field: MatchField?, edit: (MatchForm) -> MatchForm) = mutableState.update { s ->
        when (s) {
            is MatchFormState.Editing ->
                s.copy(form = edit(s.form), invalid = s.invalid?.takeIf { it.first != field }, error = null)
            else -> s
        }
    }

    /** Saves, once at a time. */
    fun save() {
        val editing = mutableState.value as? MatchFormState.Editing ?: return
        if (editing.busy) return
        mutableState.value = editing.copy(busy = true)
        viewModelScope.launch {
            mutableState.value = try {
                MatchFormState.Saved(app.core { it.saveMatch(teamId, matchId, editing.form) })
            } catch (e: MobileException.InvalidMatch) {
                editing.copy(invalid = e.field to e.reason, error = null)
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
    onSaved: (String) -> Unit,
    example: Boolean = false,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val model = viewModel { MatchFormViewModel(app, teamId, matchId, example) }
    val state by model.state.collectAsState()
    val back = rememberFormBack((state as? MatchFormState.Editing)?.changed == true, onBack)
    LaunchedEffect(state) {
        (state as? MatchFormState.Saved)?.let { onSaved(it.matchId) }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(if (matchId == null) R.string.new_match else R.string.edit_match), Modifier.title()) },
                navigationIcon = { BackButton(back) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding).imePadding()
        when (val s = state) {
            MatchFormState.Loading, is MatchFormState.Saved -> Centered(modifier) { CircularProgressIndicator() }
            is MatchFormState.Failed -> Centered(modifier) { Text(s.message) }
            is MatchFormState.Editing -> MatchFormContent(s, model, modifier)
        }
    }
}

@Composable
private fun MatchFormContent(state: MatchFormState.Editing, model: MatchFormViewModel, modifier: Modifier) =
    Column(modifier) {
        val form = state.form
        val error = { field: MatchField -> state.invalid?.takeIf { it.first == field }?.second }
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).tutorial("match-fields").padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            OutlinedTextField(
                value = form.opponent,
                onValueChange = { opponent -> model.change(MatchField.OPPONENT) { it.copy(opponent = opponent) } },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.opponent)) },
                singleLine = true,
                isError = error(MatchField.OPPONENT) != null,
                supportingText = error(MatchField.OPPONENT)?.let { { Text(it) } },
                keyboardOptions = KeyboardOptions(
                    capitalization = KeyboardCapitalization.Words,
                    imeAction = ImeAction.Done,
                ),
            )
            DateField(form.date, error(MatchField.DATE)) { date ->
                model.change(MatchField.DATE) { it.copy(date = date) }
            }
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                listOf(true to R.string.home_match, false to R.string.away_match).forEachIndexed { index, (home, name) ->
                    SegmentedButton(
                        selected = form.home == home,
                        onClick = { model.change(null) { it.copy(home = home) } },
                        shape = SegmentedButtonDefaults.itemShape(index, 2),
                    ) { Text(stringResource(name)) }
                }
            }
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
        }
        state.error?.let {
            Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
        }
        // big and at the bottom, where the thumb is
        Button(
            onClick = model::save,
            enabled = !state.busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp).tutorial("save-match"),
        ) { Text(stringResource(R.string.save)) }
    }

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
