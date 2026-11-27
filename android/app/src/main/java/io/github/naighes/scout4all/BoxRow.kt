package io.github.naighes.scout4all

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

/** The box of a row of a list, as on the desktop (`ui::box_row`): a square
 * at its head, as tall as the rows are at least, the texts, then a figure
 * at its tail. A touch anywhere opens it; without `onClick` it only shows. */
@Composable
internal fun BoxRow(
    onClick: (() -> Unit)?,
    modifier: Modifier = Modifier,
    head: @Composable BoxScope.() -> Unit,
    tail: @Composable () -> Unit = {},
    border: BorderStroke? = null,
    content: @Composable ColumnScope.() -> Unit,
) {
    val box = modifier.fillMaxWidth()
    val color = MaterialTheme.colorScheme.surfaceVariant
    // the grey is also the primary container: its content would be amber
    val contentColor = MaterialTheme.colorScheme.onSurface
    val row: @Composable () -> Unit = {
        Row(Modifier.heightIn(min = ROW_HEAD), verticalAlignment = Alignment.CenterVertically) {
            Box(Modifier.size(ROW_HEAD), contentAlignment = Alignment.Center, content = head)
            Column(
                Modifier.weight(1f).padding(horizontal = 12.dp, vertical = 6.dp),
                verticalArrangement = Arrangement.Center,
                content = content,
            )
            tail()
        }
    }
    when (onClick) {
        null -> Surface(box, ROW_SHAPE, color, contentColor, border = border, content = row)
        else -> Surface(onClick, box, shape = ROW_SHAPE, color = color, contentColor = contentColor, border = border, content = row)
    }
}

/** The figure at the tail of a `BoxRow` (a shirt number, a score), large. */
@Composable
internal fun TailFigure(text: String, color: Color, modifier: Modifier = Modifier) = Text(
    text,
    modifier.padding(end = 16.dp),
    style = MaterialTheme.typography.headlineMedium,
    fontWeight = FontWeight.Bold,
    fontStyle = FontStyle.Italic,
    color = color,
)

/** The height of a row of a list, and the side of its head. */
internal val ROW_HEAD = 72.dp

/** The corners of a `BoxRow`. */
internal val ROW_SHAPE = RoundedCornerShape(12.dp)
