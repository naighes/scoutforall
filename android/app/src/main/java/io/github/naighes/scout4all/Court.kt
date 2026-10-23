package io.github.naighes.scout4all

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.painterResource
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
    /** the server, about to serve: in the serve zone, with the ball */
    val serving: Boolean = false,
)

/** The half court of the team seen from the bench, the net at the top: the
 * attack line at 3 of the 9 meters, the front row 4 3 2 before it, the back
 * row 5 6 1 behind it. `slot` tells what position 1 to 6 shows. Wider than
 * deep, to leave room for the choices below. With `serveZone`, the zone
 * behind the end line, where the server goes to serve. */
@Composable
fun Court(
    slot: (Int) -> CourtSlot,
    onPosition: (Int) -> Unit,
    showPositions: Boolean = false,
    serveZone: Boolean = false,
) = Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp)) {
    val floor = MaterialTheme.colorScheme.surfaceVariant
    val lines = MaterialTheme.colorScheme.onSurface.copy(alpha = 0.7f)
    val net = MaterialTheme.colorScheme.onSurface
    val server = slot(1).takeIf { serveZone && it.serving }
    Box(Modifier.fillMaxWidth().aspectRatio(1.4f)) {
        Canvas(Modifier.fillMaxSize()) {
            val line = 2.dp.toPx()
            drawRect(floor)
            drawRect(lines, topLeft = Offset(line / 2, line / 2), style = Stroke(line),
                size = size.copy(width = size.width - line, height = size.height - line))
            val attack = size.height / 3
            drawLine(lines, Offset(0f, attack), Offset(size.width, attack), strokeWidth = line)
            drawLine(net, Offset(0f, 0f), Offset(size.width, 0f), strokeWidth = 4.dp.toPx())
        }
        Column(Modifier.fillMaxSize()) {
            listOf(listOf(4, 3, 2) to 1f, listOf(5, 6, 1) to 2f).forEach { (row, depth) ->
                Row(Modifier.weight(depth).fillMaxWidth()) {
                    row.forEach { position ->
                        val m = Modifier.weight(1f).fillMaxSize()
                        when {
                            // the server left the zone for the serve
                            position == 1 && server != null -> Box(m)
                            else -> Spot(position, slot(position), showPositions, m) { onPosition(position) }
                        }
                    }
                }
            }
        }
    }
    if (serveZone) {
        // behind the end line, under position 1
        Row(Modifier.fillMaxWidth().height(72.dp)) {
            Box(Modifier.weight(2f))
            when (server) {
                null -> Box(Modifier.weight(1f))
                else -> Spot(1, server, showPosition = false, Modifier.weight(1f).fillMaxSize()) { onPosition(1) }
            }
        }
    }
}

/** The zone of a position, all of it to touch, with its player in the
 * middle. */
@Composable
private fun Spot(position: Int, slot: CourtSlot, showPosition: Boolean, modifier: Modifier, onClick: () -> Unit) =
    Box(modifier.clickable(enabled = slot.enabled, onClick = onClick)) {
        if (showPosition) {
            Text(
                position.toString(),
                modifier = Modifier.align(Alignment.TopStart).padding(6.dp),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Column(
            Modifier.align(Alignment.Center).alpha(if (slot.enabled) 1f else 0.38f),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Token(slot)
            Text(
                slot.name.orEmpty(),
                modifier = Modifier.padding(horizontal = 4.dp),
                style = MaterialTheme.typography.labelSmall,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                textAlign = TextAlign.Center,
            )
        }
    }

/** The player as a round jersey with the number; an empty position asks
 * for a player, in the color of the actions. */
@Composable
private fun Token(slot: CourtSlot) = Box {
    val empty = slot.number == null
    Surface(
        modifier = Modifier.size(44.dp),
        shape = CircleShape,
        color = if (empty) MaterialTheme.colorScheme.surface else MaterialTheme.colorScheme.onSurface,
        border = when {
            slot.highlighted -> BorderStroke(3.dp, MaterialTheme.colorScheme.primary)
            empty -> BorderStroke(1.dp, MaterialTheme.colorScheme.primary)
            else -> null
        },
    ) {
        Box(contentAlignment = Alignment.Center) {
            Text(
                slot.number?.toString() ?: "+",
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.Bold,
                color = if (empty) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.surface,
            )
        }
    }
    when {
        slot.setter -> Badge(R.string.setter_short, Modifier.align(Alignment.TopEnd))
        slot.libero -> Badge(R.string.libero_short, Modifier.align(Alignment.TopEnd))
    }
    if (slot.serving) {
        Icon(
            painterResource(R.drawable.ic_ball),
            contentDescription = stringResource(R.string.serving),
            modifier = Modifier.align(Alignment.BottomStart).offset(x = (-14).dp, y = 4.dp).size(22.dp),
            tint = MaterialTheme.colorScheme.primary,
        )
    }
}

/** The setter or the libero, on the jersey. */
@Composable
private fun Badge(text: Int, modifier: Modifier) = Surface(
    modifier = modifier.size(18.dp),
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
