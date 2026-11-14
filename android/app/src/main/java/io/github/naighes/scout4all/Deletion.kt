package io.github.naighes.scout4all

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.ui.Alignment
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.height
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Delete
import androidx.compose.material.icons.filled.Warning
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Scout4All
import kotlinx.coroutines.launch

/** A deletion to confirm, as loud as it is for good: the warning in red
 * over what goes, the answer red too. */
@Composable
fun DeletionDialog(
    text: String,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
    confirm: String = stringResource(R.string.delete_forever),
    /** another way to delete, with its button over the first */
    also: Pair<String, () -> Unit>? = null,
) = AlertDialog(
    onDismissRequest = onDismiss,
    icon = { Icon(Icons.Filled.Warning, contentDescription = null, tint = MaterialTheme.colorScheme.error) },
    title = { Text(stringResource(R.string.permanent_deletion), color = MaterialTheme.colorScheme.error) },
    text = {
        Column {
            Text(text)
            Spacer(androidx.compose.ui.Modifier.height(12.dp))
            Text(
                stringResource(R.string.irreversible),
                color = MaterialTheme.colorScheme.error,
                fontWeight = FontWeight.Bold,
            )
        }
    },
    confirmButton = {
        val colors = ButtonDefaults.buttonColors(
            containerColor = MaterialTheme.colorScheme.error,
            contentColor = MaterialTheme.colorScheme.onError,
        )
        Column(horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(8.dp)) {
            also?.let { (name, onAlso) -> Button(onClick = onAlso, colors = colors) { Text(name) } }
            Button(onClick = onConfirm, colors = colors) { Text(confirm) }
        }
    },
    dismissButton = {
        TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) }
    },
)

/** What's deleted for good once confirmed, as `DeletionDialog` asks:
 * `onDeleted` after; why not, when the core says so. `asking` shows the
 * question, e.g. from a button. */
@Composable
fun Deletion(
    text: String,
    asking: Boolean,
    onAsking: (Boolean) -> Unit,
    delete: (Scout4All) -> Unit,
    onDeleted: () -> Unit,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    var error by rememberSaveable { mutableStateOf<String?>(null) }
    if (asking) {
        DeletionDialog(
            text,
            onConfirm = {
                onAsking(false)
                scope.launch {
                    try {
                        app.core(delete)
                        onDeleted()
                    } catch (e: MobileException) {
                        error = e.reason
                    }
                }
            },
            onDismiss = { onAsking(false) },
        )
    }
    error?.let {
        AlertDialog(
            onDismissRequest = { error = null },
            text = { Text(it) },
            confirmButton = { TextButton(onClick = { error = null }) { Text(stringResource(android.R.string.ok)) } },
        )
    }
}

/** The button of the top bar that deletes, once confirmed (`Deletion`). */
@Composable
fun DeleteButton(description: String, text: String, delete: (Scout4All) -> Unit, onDeleted: () -> Unit) {
    var asking by rememberSaveable { mutableStateOf(false) }
    IconButton(onClick = { asking = true }) {
        Icon(Icons.Filled.Delete, contentDescription = description)
    }
    Deletion(text, asking, { asking = it }, delete, onDeleted)
}
