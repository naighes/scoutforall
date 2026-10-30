package io.github.naighes.scout4all

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.ListItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.role
import androidx.compose.ui.semantics.selected
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import io.github.naighes.scout4all.core.CountryChoice

/** The flag of a country, from its ISO code: the two letters as regional
 * indicators, which the system draws as the flag. Not a letter code: none. */
fun flag(code: String): String = when {
    code.length == 2 && code.all { it in 'A'..'Z' } ->
        code.map { String(Character.toChars(0x1F1E6 + (it - 'A'))) }.joinToString("")
    else -> ""
}

/** The country of a team in a form: its flag and name, a touch opens the
 * list to choose it. TalkBack reads the label and the name. */
@Composable
fun CountryField(label: String, country: CountryChoice?, error: String?, onClick: () -> Unit) {
    val name = country?.name.orEmpty()
    OutlinedTextField(
        value = country?.let { "${flag(it.value)}  ${it.name}" }.orEmpty(),
        onValueChange = {},
        readOnly = true,
        enabled = false,
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClickLabel = label, onClick = onClick)
            .semantics(mergeDescendants = true) {
                role = Role.Button
                contentDescription = "$label, $name"
            },
        label = { Text(label) },
        singleLine = true,
        isError = error != null,
        supportingText = error?.let { { Text(it) } },
        // enabled = false only to take the touch: drawn as an enabled field
        colors = androidx.compose.material3.OutlinedTextFieldDefaults.colors(
            disabledTextColor = MaterialTheme.colorScheme.onSurface,
            disabledBorderColor = if (error != null) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.outline,
            disabledLabelColor = if (error != null) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
            disabledSupportingTextColor = MaterialTheme.colorScheme.error,
        ),
    )
}

/** The countries to choose from, on the whole screen: what's typed narrows
 * them (the core searches the name and the code), the chosen one ticked. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CountryDialog(title: String, current: String?, onPick: (CountryChoice) -> Unit, onDismiss: () -> Unit) =
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false)) {
        val app = LocalContext.current.applicationContext as Scout4AllApplication
        var query by rememberSaveable { mutableStateOf("") }
        val countries by produceState(emptyList<CountryChoice>(), query) {
            value = app.core { it.countries(query) }
        }
        val focus = remember { FocusRequester() }
        val list = rememberLazyListState()
        // the chosen one in sight when the list opens
        LaunchedEffect(countries.isNotEmpty()) {
            countries.indexOfFirst { it.value == current }.takeIf { query.isEmpty() && it > 0 }?.let {
                list.scrollToItem(it)
            }
        }
        LaunchedEffect(Unit) { focus.requestFocus() }
        Scaffold(
            topBar = {
                TopAppBar(
                    title = { Text(title, Modifier.title()) },
                    navigationIcon = { BackButton(onDismiss) },
                )
            },
        ) { padding ->
            Column(Modifier.fillMaxSize().padding(padding).imePadding()) {
                OutlinedTextField(
                    value = query,
                    onValueChange = { query = it },
                    modifier = Modifier.fillMaxWidth().padding(16.dp).focusRequester(focus),
                    placeholder = { Text(stringResource(R.string.search_country)) },
                    leadingIcon = { Icon(Icons.Filled.Search, contentDescription = null) },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(
                        capitalization = KeyboardCapitalization.Words,
                        imeAction = ImeAction.Search,
                    ),
                )
                if (countries.isEmpty() && query.isNotBlank()) {
                    Text(
                        stringResource(R.string.no_country_found),
                        Modifier.padding(16.dp),
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                LazyColumn(Modifier.weight(1f), state = list) {
                    items(countries, key = { it.value }) { country ->
                        val chosen = country.value == current
                        ListItem(
                            modifier = Modifier
                                .clickable { onPick(country) }
                                .semantics { selected = chosen },
                            leadingContent = { Text(flag(country.value), Modifier.semantics { contentDescription = "" }) },
                            headlineContent = { Text(country.name) },
                            trailingContent = if (chosen) {
                                { Icon(Icons.Filled.Check, contentDescription = null) }
                            } else {
                                null
                            },
                        )
                    }
                }
            }
        }
    }
