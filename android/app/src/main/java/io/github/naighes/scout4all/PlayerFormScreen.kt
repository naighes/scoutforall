package io.github.naighes.scout4all

import android.net.Uri
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.interaction.MutableInteractionSource
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.border
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.ui.draw.clip
import androidx.compose.ui.platform.testTag
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.material3.OutlinedButton
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.text.style.TextAlign
import androidx.core.content.FileProvider
import java.io.File
import java.util.UUID
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import androidx.compose.foundation.interaction.PressInteraction
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
import androidx.compose.material.icons.filled.Clear
import androidx.compose.material.icons.filled.DateRange
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.SelectableDates
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
import io.github.naighes.scout4all.core.ExistingPlayer
import io.github.naighes.scout4all.core.DominantHand
import io.github.naighes.scout4all.core.CountryChoice
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.PlayerField
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleChoice
import io.github.naighes.scout4all.core.Scout4All
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import java.time.format.FormatStyle
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
        /** another player of the team with the same name, as the core tells:
         * saved only once confirmed */
        val namesake: String? = null,
        /** the form as it was read: changed, going back asks first */
        val loaded: PlayerForm = form,
        /** the number as saved, for the removal from the registry */
        val savedNumber: Int = 0,
        /** the persons of the registry a new player may be (D5) */
        val existing: List<ExistingPlayer> = emptyList(),
        /** the other teams the player plays in, with the same details */
        val otherTeams: List<String> = emptyList(),
        /** the photo the player has now, as JPEG */
        val stored: ByteArray? = null,
        /** what the form does with the photo when saved */
        val photo: PhotoEdit = PhotoEdit.Keep,
        /** the photo being read: the face is looked for */
        val finding: Boolean = false,
    ) : PlayerFormState {
        /** the person of the registry chosen, while the name is theirs */
        val chosen: ExistingPlayer? get() = form.person?.let { p -> existing.firstOrNull { it.id == p } }

        val changed: Boolean get() = form != loaded || photo != PhotoEdit.Keep

        /** the photo the preview shows */
        val shown: ByteArray? get() = when (photo) {
            PhotoEdit.Keep -> stored
            is PhotoEdit.Replace -> photo.bytes
            PhotoEdit.Remove -> null
        }
    }
    /** saved or removed: the screen goes back */
    data object Done : PlayerFormState
}

/** What the form does with the photo of the player when saved. */
sealed interface PhotoEdit {
    data object Keep : PhotoEdit
    data object Remove : PhotoEdit
    /** the face cropped, as PNG */
    class Replace(val bytes: ByteArray) : PhotoEdit
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
                        // a new one: the first number free
                        form = player?.form ?: core.newPlayerForm(teamId),
                        roles = core.roles(),
                        savedName = player?.name.orEmpty(),
                        savedNumber = player?.number?.toInt() ?: 0,
                        existing = if (playerId == null) core.existingPlayers(teamId) else emptyList(),
                        otherTeams = playerId?.let { core.otherTeams(teamId, it) }.orEmpty(),
                        stored = playerId?.let { core.personPhoto(it) },
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
    fun change(field: PlayerField?, edit: (PlayerForm) -> PlayerForm) = update { s ->
        s.copy(form = edit(s.form), invalid = s.invalid?.takeIf { it.first != field }, error = null)
    }

    /** The name written: a person of the registry chosen stays only while
     * it's their name; another name is a new person. */
    fun name(name: String) = update { s ->
        val person = s.chosen?.takeIf { it.name == name }?.id
        // a new player: the photo shown is the one of the person chosen, if any
        val stored = if (person == null && s.form.person != null) null else s.stored
        s.copy(form = s.form.copy(name = name, person = person), stored = stored, invalid = s.invalid?.takeIf { it.first != PlayerField.NAME }, error = null)
    }

    /** A person of the registry picked by their label: the form with their
     * details, the number and the role of their most recent team. */
    fun pickExisting(label: String) {
        val choice = (mutableState.value as? PlayerFormState.Editing)?.existing?.firstOrNull { it.label == label } ?: return
        update { s -> s.copy(form = choice.form, stored = null, invalid = null, error = null) }
        // the photo of the person, shown as the preview
        viewModelScope.launch {
            val photo = try {
                app.core { core -> core.personPhoto(choice.id.toString()) }
            } catch (e: MobileException) {
                null
            }
            update { s -> if (s.form.person == choice.id) s.copy(stored = photo) else s }
        }
    }

    /** Removes the player from the registry, so from every team (D16). */
    fun deletePerson() = playerId?.let { id -> submit { core -> core.deletePerson(id) } }

    fun confirmRemoval(confirming: Boolean) = update { it.copy(confirmingRemoval = confirming) }

    /** The face the picture holds becomes the photo, shown at once and
     * stored when the form is saved; no face, many faces or no image are
     * errors of the form. */
    fun pickPhoto(bytes: ByteArray) {
        update { it.copy(finding = true, error = null) }
        viewModelScope.launch {
            val face = try {
                Result.success(app.core { it.cropFace(bytes) })
            } catch (e: MobileException) {
                Result.failure(e)
            }
            update { s ->
                val failure = face.exceptionOrNull()
                when {
                    failure == null -> s.copy(finding = false, photo = PhotoEdit.Replace(face.getOrThrow()), error = null)
                    failure is MobileException.NoFace -> s.copy(finding = false, error = app.getString(R.string.no_face))
                    failure is MobileException.ManyFaces ->
                        s.copy(finding = false, error = app.getString(R.string.many_faces, failure.count.toInt()))
                    else -> s.copy(finding = false, error = app.getString(R.string.file_not_read))
                }
            }
        }
    }

    fun removePhoto() = update { it.copy(photo = PhotoEdit.Remove, error = null) }

    fun save(namesakeConfirmed: Boolean = false) = submit { core ->
        val editing = mutableState.value as PlayerFormState.Editing
        val id = core.savePlayer(teamId, playerId, editing.form, namesakeConfirmed)
        // the id of the player is the one of the person
        when (val photo = editing.photo) {
            PhotoEdit.Keep -> {}
            PhotoEdit.Remove -> core.setPersonPhoto(id, null)
            is PhotoEdit.Replace -> core.setPersonPhoto(id, photo.bytes)
        }
    }

    fun dismissNamesake() = update { it.copy(namesake = null) }

    fun remove() = playerId?.let { id -> submit { core -> core.removePlayer(teamId, id) } }

    /** Saves or removes, once at a time. */
    private fun submit(call: (Scout4All) -> Unit) {
        val editing = (mutableState.value as? PlayerFormState.Editing)?.copy(namesake = null) ?: return
        if (editing.busy) return
        mutableState.value = editing.copy(busy = true, confirmingRemoval = false, namesake = null)
        viewModelScope.launch {
            mutableState.value = try {
                app.core(call)
                PlayerFormState.Done
            } catch (e: MobileException.Namesake) {
                editing.copy(namesake = e.reason, invalid = null, error = null, confirmingRemoval = false)
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
                title = { Text(stringResource(if (playerId == null) R.string.new_player else R.string.edit_player), Modifier.title()) },
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
                if (s.confirmingRemoval) RemovalDialog(s, model)
                s.namesake?.let { NamesakeDialog(it, model) }
            }
        }
    }
}

@Composable
private fun PlayerFormContent(state: PlayerFormState.Editing, model: PlayerFormViewModel, modifier: Modifier) =
    Column(modifier) {
        val form = state.form
        val app = LocalContext.current.applicationContext as Scout4AllApplication
        var choosingNationality by rememberSaveable { mutableStateOf(false) }
        val error = { field: PlayerField -> state.invalid?.takeIf { it.first == field }?.second }
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            PhotoPanel(state.shown, state.finding, model::pickPhoto, model::removePhoto)
            val nationalityName by androidx.compose.runtime.produceState("", form.nationality) {
                value = form.nationality?.let { code -> app.core { it.countryName(code) } }.orEmpty()
            }
            val nationality = form.nationality?.let { code ->
                CountryChoice(code, nationalityName)
            }
            CountryField(stringResource(R.string.nationality), nationality, null) { choosingNationality = true }
            if (choosingNationality) CountryDialog(
                stringResource(R.string.nationality), form.nationality,
                onPick = { choice ->
                    model.change(null) { it.copy(nationality = choice.value) }
                    choosingNationality = false
                },
                onDismiss = { choosingNationality = false },
            )
            when (state.existing.isEmpty()) {
                // the details are the person's, the same in every team
                true -> OutlinedTextField(
                    value = form.name,
                    onValueChange = model::name,
                    modifier = Modifier.fillMaxWidth(),
                    label = { Text(stringResource(R.string.name)) },
                    singleLine = true,
                    isError = error(PlayerField.NAME) != null,
                    supportingText = (error(PlayerField.NAME) ?: playsAlsoIn(state.otherTeams))?.let { { Text(it) } },
                    keyboardOptions = KeyboardOptions(
                        capitalization = KeyboardCapitalization.Words,
                        imeAction = ImeAction.Next,
                    ),
                )
                // a new player may be a person of the registry
                false -> SuggestField(
                    form.name,
                    stringResource(R.string.name),
                    remember(state.existing) { state.existing.map { it.label } },
                    error(PlayerField.NAME),
                    onChange = model::name,
                    onPick = model::pickExisting,
                    hint = when (state.chosen) {
                        null -> stringResource(R.string.name_or_existing)
                        else -> stringResource(R.string.in_registry)
                    },
                )
            }
            OutlinedTextField(
                value = form.number,
                onValueChange = { number -> model.change(PlayerField.NUMBER) { it.copy(number = number) } },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.number)) },
                placeholder = { Text(stringResource(R.string.number_placeholder)) },
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
            // optional, after what scouting needs
            // a player is rarely a child: the calendar opens years back, at
            // most today
            val today = remember { LocalDate.now() }
            DayField(
                stringResource(R.string.birth_date),
                runCatching { LocalDate.parse(form.birthDate) }.getOrNull(),
                onDay = { day -> model.change(PlayerField.BIRTH_DATE) { it.copy(birthDate = day?.toString().orEmpty()) } },
                otherwise = form.birthDate,
                error = error(PlayerField.BIRTH_DATE),
                clear = stringResource(R.string.remove_birth_date),
                latest = today,
                opensAt = today.minusYears(BIRTH_YEARS_SHOWN),
                firstYear = FIRST_BIRTH_YEAR,
            )
            OutlinedTextField(
                value = form.heightCm,
                onValueChange = { height -> model.change(PlayerField.HEIGHT) { it.copy(heightCm = height) } },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.height)) },
                placeholder = { Text(stringResource(R.string.height_placeholder)) },
                suffix = { Text(stringResource(R.string.centimetres)) },
                singleLine = true,
                isError = error(PlayerField.HEIGHT) != null,
                supportingText = error(PlayerField.HEIGHT)?.let { { Text(it) } },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Next),
            )
            OutlinedTextField(
                value = form.federationId,
                onValueChange = { value -> model.change(null) { it.copy(federationId = value) } },
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.federation_id)) },
                singleLine = true,
            )
            FieldTitle(stringResource(R.string.dominant_hand), null)
            FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                listOf(DominantHand.RIGHT to R.string.right_hand, DominantHand.LEFT to R.string.left_hand).forEach { (hand, label) ->
                    FilterChip(
                        selected = form.dominantHand == hand,
                        onClick = { model.change(null) { it.copy(dominantHand = if (form.dominantHand == hand) null else hand) } },
                        label = { Text(stringResource(label)) },
                    )
                }
            }
            NotesField(form.notes) { notes -> model.change(null) { it.copy(notes = notes) } }
        }
        state.error?.let {
            Text(it, Modifier.padding(horizontal = 16.dp), color = MaterialTheme.colorScheme.error)
        }
        // big and at the bottom, where the thumb is
        Button(
            onClick = { model.save() },
            enabled = !state.busy,
            modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 56.dp),
        ) { Text(stringResource(R.string.save)) }
    }

/** The photo with its preview, the buttons to choose it (gallery, files),
 * take it with the camera or remove it. */
@Composable
private fun PhotoPanel(shown: ByteArray?, finding: Boolean, onPick: (ByteArray) -> Unit, onRemove: () -> Unit) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        uri ?: return@rememberLauncherForActivityResult
        scope.launch { onPick(readUri(context, uri)) }
    }
    // the camera writes where the app says (the provider of the cache): no
    // permission needed, the camera app does it
    var taken by remember { mutableStateOf<Pair<File, Uri>?>(null) }
    val take = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { done ->
        val file = taken?.first
        taken = null
        if (file != null) {
            scope.launch {
                if (done) onPick(readUri(context, Uri.fromFile(file)))
                withContext(Dispatchers.IO) { file.delete() }
            }
        }
    }
    // the face in a circle, centred, its buttons right below
    Column(
        Modifier.fillMaxWidth().testTag(PHOTO_PANEL),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Box(
            Modifier.size(PHOTO_SIZE).clip(CircleShape).border(1.dp, MaterialTheme.colorScheme.outlineVariant, CircleShape)
                .testTag(PHOTO_CIRCLE),
            contentAlignment = Alignment.Center,
        ) {
            when {
                finding -> CircularProgressIndicator()
                shown == null ->
                    Text(stringResource(R.string.no_photo), style = MaterialTheme.typography.bodySmall, textAlign = TextAlign.Center)
                else -> LogoBox(shown, PHOTO_SIZE)
            }
        }
        FlowRow(
            horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.CenterHorizontally),
            verticalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            OutlinedButton(onClick = { pick.launch("image/*") }, enabled = !finding) {
                Text(stringResource(R.string.choose_photo))
            }
            OutlinedButton(
                onClick = {
                    val file = File(File(context.cacheDir, "photos").apply { mkdirs() }, "${UUID.randomUUID()}.jpg")
                    val uri = FileProvider.getUriForFile(context, "${context.packageName}.files", file)
                    taken = file to uri
                    take.launch(uri)
                },
                enabled = !finding,
            ) { Text(stringResource(R.string.take_photo)) }
        }
        if (shown != null) {
            TextButton(onClick = onRemove) { Text(stringResource(R.string.remove_photo)) }
        }
    }
}

/** The side of the photo in the form of the player. */
private val PHOTO_SIZE = 160.dp

/** Tags of the photo panel and of its circle (tests). */
const val PHOTO_PANEL = "photo_panel"
const val PHOTO_CIRCLE = "photo_circle"

/** "Also plays in: …", when they do. */
@Composable
private fun playsAlsoIn(teams: List<String>): String? =
    teams.takeIf { it.isNotEmpty() }?.let { stringResource(R.string.plays_also_in, it.joinToString(", ")) }

/** Out of the team (they stay in the registry), or out of the registry,
 * so of every team (D16): one question, two answers. */
@Composable
private fun RemovalDialog(state: PlayerFormState.Editing, model: PlayerFormViewModel) = DeletionDialog(
    listOfNotNull(
        stringResource(R.string.delete_player_text, state.savedName),
        playsAlsoIn(state.otherTeams),
    ).joinToString("\n") + "\n\n" + stringResource(R.string.delete_person_text, state.savedNumber),
    onConfirm = { model.remove() },
    onDismiss = { model.confirmRemoval(false) },
    confirm = stringResource(R.string.remove_from_team),
    also = stringResource(R.string.delete_person) to { model.deletePerson() },
)

/** Another player of the team with the same name: saved all the same, or
 * back to the form. */
@Composable
private fun NamesakeDialog(namesake: String, model: PlayerFormViewModel) = AlertDialog(
    onDismissRequest = model::dismissNamesake,
    title = { Text(stringResource(R.string.same_name)) },
    text = { Text(namesake) },
    confirmButton = {
        Button(onClick = { model.save(namesakeConfirmed = true) }) { Text(stringResource(R.string.save_anyway)) }
    },
    dismissButton = {
        TextButton(onClick = model::dismissNamesake) { Text(stringResource(R.string.cancel)) }
    },
)

/** The years back the calendar opens at without a birth date. */
private const val BIRTH_YEARS_SHOWN = 18L
private const val FIRST_BIRTH_YEAR = 1900

/** Notes, on more lines: of a player or of a match. */
@Composable
fun NotesField(notes: String, onNotes: (String) -> Unit) = OutlinedTextField(
    value = notes,
    onValueChange = onNotes,
    modifier = Modifier.fillMaxWidth(),
    label = { Text(stringResource(R.string.notes)) },
    minLines = 3,
    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
)

