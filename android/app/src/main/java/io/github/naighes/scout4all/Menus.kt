package io.github.naighes.scout4all

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.material3.DropdownMenu
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
