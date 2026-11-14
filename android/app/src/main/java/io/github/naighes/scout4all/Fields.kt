package io.github.naighes.scout4all

import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Checkbox
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ExposedDropdownMenuAnchorType
import androidx.compose.material3.ExposedDropdownMenuBox
import androidx.compose.material3.ExposedDropdownMenuDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.offeredNames

/** A text with what may be meant offered below as it's written, as on the
 * desktop: a touch picks one (`onPick` with its text among `offered`),
 * Done on the keyboard the first. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SuggestField(
    value: String,
    label: String,
    offered: List<String>,
    error: String?,
    onChange: (String) -> Unit,
    onPick: (String) -> Unit,
    modifier: Modifier = Modifier,
    hint: String? = null,
    placeholder: String? = null,
) {
    var open by remember { mutableStateOf(false) }
    var field by remember { mutableStateOf(TextFieldValue(value, TextRange(value.length))) }
    // a text from outside (a pick): the cursor at its end, not where it was
    val current = when (field.text == value) {
        true -> field
        false -> TextFieldValue(value, TextRange(value.length))
    }
    // the ones that begin as written first, as the core orders them
    val shown = remember(value, offered) { offeredNames(offered, value).take(6) }
    val expanded = open && shown.isNotEmpty()
    ExposedDropdownMenuBox(expanded = expanded, onExpandedChange = { open = it }, modifier = modifier) {
        OutlinedTextField(
            value = current,
            onValueChange = {
                field = it
                if (it.text != value) {
                    onChange(it.text)
                    open = true
                }
            },
            modifier = Modifier.fillMaxWidth().menuAnchor(ExposedDropdownMenuAnchorType.PrimaryEditable),
            label = { Text(label) },
            placeholder = placeholder?.let { { Text(it) } },
            singleLine = true,
            isError = error != null,
            supportingText = (error ?: hint)?.let { { Text(it) } },
            keyboardOptions = KeyboardOptions(
                capitalization = KeyboardCapitalization.Words,
                imeAction = ImeAction.Done,
            ),
            keyboardActions = KeyboardActions(onDone = {
                if (expanded) shown.firstOrNull()?.let(onPick)
                open = false
                defaultKeyboardAction(ImeAction.Done)
            }),
        )
        ExposedDropdownMenu(expanded = expanded, onDismissRequest = { open = false }) {
            shown.forEach { name ->
                DropdownMenuItem(
                    text = { Text(name) },
                    onClick = {
                        onPick(name)
                        open = false
                    },
                    contentPadding = ExposedDropdownMenuDefaults.ItemContentPadding,
                )
            }
        }
    }
}

/** One of `options` (value and name), from a menu. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun <T> ChoiceField(
    label: String,
    options: List<Pair<T, String>>,
    selected: T,
    onSelect: (T) -> Unit,
    modifier: Modifier = Modifier,
) {
    var open by remember { mutableStateOf(false) }
    ExposedDropdownMenuBox(expanded = open, onExpandedChange = { open = it }, modifier = modifier) {
        OutlinedTextField(
            value = options.firstOrNull { it.first == selected }?.second.orEmpty(),
            onValueChange = {},
            readOnly = true,
            modifier = Modifier.fillMaxWidth().menuAnchor(ExposedDropdownMenuAnchorType.PrimaryNotEditable),
            label = { Text(label) },
            singleLine = true,
            trailingIcon = { ExposedDropdownMenuDefaults.TrailingIcon(expanded = open) },
        )
        ExposedDropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            options.forEach { (value, name) ->
                DropdownMenuItem(
                    text = { Text(name) },
                    onClick = {
                        onSelect(value)
                        open = false
                    },
                    contentPadding = ExposedDropdownMenuDefaults.ItemContentPadding,
                )
            }
        }
    }
}

/** A yes or no, the whole row to touch. */
@Composable
fun CheckRow(text: String, checked: Boolean, onChecked: (Boolean) -> Unit, modifier: Modifier = Modifier) = Row(
    modifier
        .fillMaxWidth()
        .heightIn(min = 48.dp)
        .toggleable(value = checked, role = Role.Checkbox, onValueChange = onChecked),
    verticalAlignment = Alignment.CenterVertically,
) {
    Checkbox(checked = checked, onCheckedChange = null, modifier = Modifier.padding(end = 12.dp))
    Text(text, style = MaterialTheme.typography.bodyLarge)
}

/** The same name, as the desktop links them: whatever the case and the
 * spaces around. */
fun sameName(a: String, b: String) = a.trim().lowercase() == b.trim().lowercase()

/** "–": no choice, in a menu. */
const val NO_CHOICE = "–"
