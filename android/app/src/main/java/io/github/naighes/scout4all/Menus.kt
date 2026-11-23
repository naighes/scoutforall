package io.github.naighes.scout4all

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Box
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.graphics.painter.Painter
import androidx.compose.ui.graphics.vector.rememberVectorPainter
import androidx.compose.ui.res.painterResource
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp

/** The fill and the edge of a menu: lighter than the screen, with a line
 * around it. Material fills it with `surfaceContainer`, the color of the
 * screen in this theme: the menu melted into what it covers. */
internal object MenuColors {
    val container = Scout4AllColors.surfaceContainerHigh
    val edge = Color(0xFF6E6E72)
}

/** A menu over the screen, as the share one of the top bar or a filter. */
@Composable
fun Menu(expanded: Boolean, onDismissRequest: () -> Unit, content: @Composable ColumnScope.() -> Unit) =
    DropdownMenu(
        expanded = expanded,
        onDismissRequest = onDismissRequest,
        shape = MaterialTheme.shapes.medium,
        containerColor = MenuColors.container,
        border = BorderStroke(1.dp, MenuColors.edge),
        shadowElevation = 8.dp,
        content = content,
    )

/** The three dots of the top bar: the actions a screen seldom needs (export,
 * edit, delete), so that the bar keeps the ones in use. `content` gets what
 * closes the menu, to call once an entry is chosen. */
@Composable
fun MoreMenu(content: @Composable ColumnScope.(close: () -> Unit) -> Unit) = Box {
    var open by rememberSaveable { mutableStateOf(false) }
    IconButton(onClick = { open = true }) {
        Icon(Icons.Filled.MoreVert, contentDescription = stringResource(R.string.more_actions))
    }
    Menu(expanded = open, onDismissRequest = { open = false }) { content { open = false } }
}

/** An entry of `MoreMenu`: closes the menu, then acts. */
@Composable
fun MenuEntry(text: String, icon: Painter, close: () -> Unit, onClick: () -> Unit) =
    DropdownMenuItem(
        text = { Text(text) },
        leadingIcon = { Icon(icon, contentDescription = null) },
        onClick = {
            close()
            onClick()
        },
    )

/** An entry of `MoreMenu` with an icon of Material. */
@Composable
fun MenuEntry(text: String, icon: androidx.compose.ui.graphics.vector.ImageVector, close: () -> Unit, onClick: () -> Unit) =
    MenuEntry(text, rememberVectorPainter(icon), close, onClick)
