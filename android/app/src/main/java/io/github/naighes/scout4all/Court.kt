package io.github.naighes.scout4all

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp

/** What a position of the court shows. */
data class CourtSlot(
    /** the player, if any: an empty position asks for one */
    val number: Int?,
    val name: String?,
    val setter: Boolean = false,
    val libero: Boolean = false,
    /** being chosen, in the color of the actions */
    val highlighted: Boolean = false,
    /** a position that can't be touched now is dimmed */
    val enabled: Boolean = true,
)

/** The court seen from the bench, the net at the top: front row 4 3 2,
 * back row 5 6 1. `slot` tells what position 1 to 6 shows. */
@Composable
fun Court(slot: (Int) -> CourtSlot, onPosition: (Int) -> Unit) = Column(
    Modifier.fillMaxWidth().padding(horizontal = 16.dp),
    verticalArrangement = Arrangement.spacedBy(8.dp),
) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, modifier = Modifier.fillMaxWidth()) {
        Text(
            stringResource(R.string.net),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        HorizontalDivider(thickness = 2.dp, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
    listOf(listOf(4, 3, 2), listOf(5, 6, 1)).forEach { row ->
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            row.forEach { position ->
                CourtCell(position, slot(position), Modifier.weight(1f)) { onPosition(position) }
            }
        }
    }
}

@Composable
private fun CourtCell(position: Int, slot: CourtSlot, modifier: Modifier, onClick: () -> Unit) = Surface(
    // a bit wider than tall: room for the choices below
    modifier = modifier.aspectRatio(1.25f).clickable(enabled = slot.enabled, onClick = onClick),
    shape = MaterialTheme.shapes.medium,
    color = MaterialTheme.colorScheme.surfaceVariant,
    border = if (slot.highlighted) BorderStroke(2.dp, MaterialTheme.colorScheme.primary) else null,
) {
    Box(Modifier.padding(8.dp).alpha(if (slot.enabled) 1f else 0.38f)) {
        Text(
            stringResource(R.string.position_n, position),
            modifier = Modifier.align(Alignment.TopStart),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        when {
            slot.setter -> Badge(R.string.setter_short, Modifier.align(Alignment.TopEnd))
            slot.libero -> Badge(R.string.libero_short, Modifier.align(Alignment.TopEnd))
        }
        when (slot.number) {
            // an empty position asks for a player, in the color of the actions
            null -> Text(
                "+",
                modifier = Modifier.align(Alignment.Center),
                style = MaterialTheme.typography.headlineMedium,
                color = MaterialTheme.colorScheme.primary,
            )
            else -> Column(
                Modifier.align(Alignment.Center).padding(top = 8.dp),
                horizontalAlignment = Alignment.CenterHorizontally,
            ) {
                Text(
                    slot.number.toString(),
                    style = MaterialTheme.typography.headlineMedium,
                    fontWeight = FontWeight.SemiBold,
                )
                Text(
                    slot.name.orEmpty(),
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    textAlign = TextAlign.Center,
                )
            }
        }
    }
}

/** The setter or the libero, in a corner of the cell. */
@Composable
private fun Badge(text: Int, modifier: Modifier) = Surface(
    modifier = modifier.width(20.dp).height(20.dp),
    shape = CircleShape,
    color = MaterialTheme.colorScheme.primary,
) {
    Box(contentAlignment = Alignment.Center) {
        Text(
            stringResource(text),
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.Bold,
            color = MaterialTheme.colorScheme.onPrimary,
        )
    }
}
