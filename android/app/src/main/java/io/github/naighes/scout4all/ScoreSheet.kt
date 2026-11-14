package io.github.naighes.scout4all

import androidx.compose.animation.animateColorAsState
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.CheckCircle
import androidx.compose.material.icons.filled.Lock
import androidx.compose.material.icons.outlined.Edit
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.RadioButton
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.selectableGroup
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.RoleChoice
import io.github.naighes.scout4all.core.RoleEnum

/** A player as a tile of a score sheet (of a match, of a new season): the
 * number big, as on the shirt, the name below, then `details`. Chosen,
 * it's lit with a tick, or a lock when it can't be taken off; the pencil
 * in the corner corrects number and role. */
@Composable
fun PlayerTile(
    number: Int,
    name: String,
    details: List<String>,
    chosen: Boolean,
    locked: Boolean,
    onCorrect: () -> Unit,
    onToggle: () -> Unit,
) {
    val colors = MaterialTheme.colorScheme
    val container by animateColorAsState(if (chosen) colors.primaryContainer else colors.surfaceContainerHigh, label = "tile")
    val content = if (chosen) colors.onPrimaryContainer else colors.onSurfaceVariant
    Surface(
        shape = MaterialTheme.shapes.large,
        color = container,
        contentColor = content,
        border = if (chosen) BorderStroke(2.dp, colors.primary) else null,
        modifier = Modifier
            .fillMaxWidth()
            .heightIn(min = 120.dp)
            .clip(MaterialTheme.shapes.large)
            .toggleable(value = chosen, enabled = !locked, role = Role.Checkbox, onValueChange = { onToggle() }),
    ) {
        Box {
            Column(
                Modifier.fillMaxWidth().padding(horizontal = 8.dp, vertical = 12.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                Text(number.toString(), style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
                Text(name, style = MaterialTheme.typography.bodyMedium, textAlign = TextAlign.Center)
                if (details.isNotEmpty()) {
                    Text(
                        details.joinToString(" · "),
                        style = MaterialTheme.typography.labelSmall,
                        textAlign = TextAlign.Center,
                        color = content.copy(alpha = 0.75f),
                    )
                }
            }
            IconButton(onClick = onCorrect, modifier = Modifier.align(Alignment.TopStart)) {
                Icon(
                    Icons.Outlined.Edit,
                    contentDescription = "$number $name: ${stringResource(R.string.roster_correct)}",
                    modifier = Modifier.size(18.dp),
                )
            }
            if (chosen) {
                Icon(
                    if (locked) Icons.Filled.Lock else Icons.Filled.CheckCircle,
                    contentDescription = null,
                    tint = colors.primary,
                    modifier = Modifier.align(Alignment.TopEnd).padding(12.dp).size(20.dp),
                )
            }
        }
    }
}

/** The number and the role of a player, "number name" as the `title`; an
 * error of the core stays under the number. The roles one per row: chips
 * in a `FlowRow` made the dialog measure for ever. It scrolls on a small
 * screen. */
@Composable
fun NumberRoleDialog(
    title: String,
    number: String,
    role: RoleEnum?,
    roles: List<RoleChoice>,
    error: String?,
    onNumber: (String) -> Unit,
    onRole: (RoleEnum?) -> Unit,
    onOk: () -> Unit,
    onCancel: () -> Unit,
) = Dialog(onDismissRequest = onCancel, properties = DialogProperties(usePlatformDefaultWidth = false)) {
    Surface(
        Modifier.padding(24.dp).widthIn(max = 560.dp),
        shape = MaterialTheme.shapes.extraLarge,
        tonalElevation = 6.dp,
    ) {
        Column(
            Modifier.verticalScroll(rememberScrollState()).padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(title, Modifier.title(), style = MaterialTheme.typography.headlineSmall)
            OutlinedTextField(
                value = number,
                onValueChange = onNumber,
                modifier = Modifier.fillMaxWidth(),
                label = { Text(stringResource(R.string.number)) },
                singleLine = true,
                isError = error != null,
                supportingText = error?.let { { Text(it) } },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
            )
            FieldTitle(stringResource(R.string.role), null)
            Column(Modifier.selectableGroup()) {
                (listOf<Pair<RoleEnum?, String>>(null to NO_CHOICE) + roles.map { it.value to it.name })
                    .forEach { (value, name) ->
                        Row(
                            Modifier
                                .fillMaxWidth()
                                .heightIn(min = 48.dp)
                                .selectable(selected = role == value, role = Role.RadioButton, onClick = { onRole(value) }),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            RadioButton(selected = role == value, onClick = null, modifier = Modifier.padding(end = 12.dp))
                            Text(name, style = MaterialTheme.typography.bodyLarge)
                        }
                    }
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                TextButton(onClick = onCancel) { Text(stringResource(R.string.cancel)) }
                TextButton(onClick = onOk) { Text(stringResource(R.string.ok)) }
            }
        }
    }
}
