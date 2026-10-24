package io.github.naighes.scout4all

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
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
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
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
    /** where the player stands, in meters from the left sideline and from
     * the net; without, in the middle of the zone of the position */
    val x: Float? = null,
    val y: Float? = null,
    /** the same player moves from a spot to the next one */
    val key: Any? = null,
    /** the last touch of the rally: the symbol of its evaluation, empty
     * without one (a freeball) */
    val touch: String? = null,
)

/** Where the ball is: on our side, by the player who touched it last, or
 * in the middle; or on the net on their side, over the player who sent it.
 * A freeball shows the way it goes. */
data class CourtBall(
    val ours: Boolean,
    /** the position of the player who touched it last, if any */
    val position: Int? = null,
    val freeball: Boolean = false,
)

/** The middle of the zone of positions 1 to 6, in meters: the front row in
 * the 3 meters before the attack line, the back row in the 6 behind it. */
private val zones = listOf(7.5f to 6f, 7.5f to 1.5f, 4.5f to 1.5f, 1.5f to 1.5f, 1.5f to 6f, 4.5f to 6f)

/** The half court of the team seen from the bench, the net at the top: the
 * attack line at 3 of the 9 meters, the front row 4 3 2 before it, the back
 * row 5 6 1 behind it. `slot` tells what position 1 to 6 shows, and where:
 * a player moving to another spot slides there. Wider than
 * deep, to leave room for the choices below. With `serveZone`, the zone
 * behind the end line, where the server goes to serve. */
@Composable
fun Court(
    slot: (Int) -> CourtSlot,
    onPosition: (Int) -> Unit,
    showPositions: Boolean = false,
    serveZone: Boolean = false,
    ball: CourtBall? = null,
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
        BoxWithConstraints(Modifier.fillMaxSize()) {
            // the player on the spot, the name under the jersey, a third of
            // the court wide; on a court narrower than a phone's (360dp),
            // all of it smaller, so that the players don't overlap
            val scale = (maxWidth / 360.dp).coerceAtMost(1f)
            val width = maxWidth / 3
            val token = 44.dp * scale
            val height = token + 2.dp + 16.dp * scale
            // the top left corner of the spot of a position
            val corner = { position: Int ->
                val s = slot(position)
                val (zx, zy) = zones[position - 1]
                (maxWidth * (s.x ?: zx) / 9f).coerceIn(width / 2, maxWidth - width / 2) - width / 2 to
                    (maxHeight * (s.y ?: zy) / 9f).coerceIn(token / 2, maxHeight - height + token / 2) - token / 2
            }
            // the ball by the jersey, on the right; or in the middle of the
            // court, between the rows; or on the net, on their side
            val ballSize = 20.dp * scale
            val holder = ball?.position?.let(corner)
            val ballX by animateDpAsState(
                when (holder) {
                    null -> maxWidth / 2
                    else -> holder.first + width / 2 + if (ball?.ours == true) token / 2 else 0.dp
                },
                tween(450), label = "ball x",
            )
            val ballY by animateDpAsState(
                when {
                    ball?.ours != true -> (-6).dp
                    holder != null -> holder.second + token - 7.dp
                    else -> maxHeight * 4f / 9f
                },
                tween(450), label = "ball y",
            )
            if (ball?.freeball == true) {
                // from the player over the net, or from the net into the court
                val from = when (holder) {
                    null -> maxWidth / 2 to 0.dp
                    else -> holder.first + width / 2 to holder.second + token / 2
                }
                Freeball(from, ballX to ballY, MaterialTheme.colorScheme.primary)
            }
            (1..6).forEach { position ->
                val s = slot(position)
                // the server left the court for the serve
                if (position == 1 && server != null) return@forEach
                key(s.key ?: position) {
                    val (cx, cy) = corner(position)
                    val x by animateDpAsState(cx, tween(450), label = "x")
                    val y by animateDpAsState(cy, tween(450), label = "y")
                    Spot(position, s, showPositions, scale, Modifier.offset(x, y).size(width, height)) { onPosition(position) }
                }
            }
            // the server holds the ball in the serve zone
            if (ball != null && server == null) {
                Icon(
                    painterResource(R.drawable.ic_ball),
                    contentDescription = stringResource(if (ball.ours) R.string.ball_ours else R.string.ball_theirs),
                    modifier = Modifier.offset(ballX - ballSize / 2, ballY - ballSize / 2).size(ballSize),
                    tint = MaterialTheme.colorScheme.primary,
                )
            }
        }
    }
    if (serveZone) {
        // behind the end line, under position 1
        Row(Modifier.fillMaxWidth().height(72.dp)) {
            Box(Modifier.weight(2f))
            when (server) {
                null -> Box(Modifier.weight(1f))
                else -> Spot(1, server, showPosition = false, scale = 1f, Modifier.weight(1f).fillMaxSize()) { onPosition(1) }
            }
        }
    }
}

/** The player of a position, all of it to touch: the jersey at the top,
 * the name under it. */
@Composable
private fun Spot(position: Int, slot: CourtSlot, showPosition: Boolean, scale: Float, modifier: Modifier, onClick: () -> Unit) =
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
            Modifier.align(Alignment.TopCenter).alpha(if (slot.enabled) 1f else 0.38f),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.spacedBy(2.dp),
        ) {
            Token(slot, scale)
            val label = MaterialTheme.typography.labelSmall
            Text(
                slot.name.orEmpty(),
                modifier = Modifier.padding(horizontal = 4.dp),
                style = label.copy(fontSize = label.fontSize * scale, lineHeight = label.lineHeight * scale),
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                textAlign = TextAlign.Center,
            )
        }
    }

/** The player as a round jersey with the number; the libero in a jersey of
 * another color, dark with a light border; an empty position asks for a
 * player, in the color of the actions. */
@Composable
private fun Token(slot: CourtSlot, scale: Float) = Box {
    val empty = slot.number == null
    val libero = slot.libero && !empty
    val jersey = when {
        empty -> MaterialTheme.colorScheme.surface
        libero -> MaterialTheme.colorScheme.background
        else -> MaterialTheme.colorScheme.onSurface
    }
    val number = when {
        empty -> MaterialTheme.colorScheme.primary
        libero -> MaterialTheme.colorScheme.onSurface
        else -> MaterialTheme.colorScheme.surface
    }
    val liberoName = stringResource(R.string.libero)
    Surface(
        modifier = Modifier.size(44.dp * scale).semantics { if (libero) stateDescription = liberoName },
        shape = CircleShape,
        color = jersey,
        border = when {
            slot.highlighted -> BorderStroke(3.dp, MaterialTheme.colorScheme.primary)
            slot.touch != null -> BorderStroke(2.dp, MaterialTheme.colorScheme.primary.copy(alpha = 0.6f))
            empty -> BorderStroke(1.dp, MaterialTheme.colorScheme.primary)
            libero -> BorderStroke(2.dp, MaterialTheme.colorScheme.onSurface)
            else -> null
        },
    ) {
        Box(contentAlignment = Alignment.Center) {
            val title = MaterialTheme.typography.titleLarge
            Text(
                slot.number?.toString() ?: "+",
                style = title.copy(fontSize = title.fontSize * scale),
                fontWeight = FontWeight.Bold,
                color = number,
            )
        }
    }
    if (slot.setter) Badge(stringResource(R.string.setter_short), Modifier.align(Alignment.TopEnd))
    if (!slot.touch.isNullOrEmpty()) Badge(slot.touch, Modifier.align(Alignment.BottomStart).offset(x = (-4).dp, y = 4.dp))
    if (slot.serving) {
        Icon(
            painterResource(R.drawable.ic_ball),
            contentDescription = stringResource(R.string.serving),
            modifier = Modifier.align(Alignment.BottomStart).offset(x = (-14).dp, y = 4.dp).size(22.dp),
            tint = MaterialTheme.colorScheme.primary,
        )
    }
}

/** On the jersey: the setter, the evaluation of the last touch. */
@Composable
private fun Badge(text: String, modifier: Modifier) = Surface(
    modifier = modifier.size(18.dp),
    shape = CircleShape,
    color = MaterialTheme.colorScheme.primary,
) {
    Box(contentAlignment = Alignment.Center) {
        Text(
            text,
            style = MaterialTheme.typography.labelSmall,
            fontWeight = FontWeight.Bold,
            color = MaterialTheme.colorScheme.onPrimary,
        )
    }
}

/** The way of a freeball: a dashed line with its head on the ball. */
@Composable
private fun Freeball(from: Pair<Dp, Dp>, to: Pair<Dp, Dp>, color: Color) = Canvas(Modifier.fillMaxSize()) {
    val start = Offset(from.first.toPx(), from.second.toPx())
    val end = Offset(to.first.toPx(), to.second.toPx())
    val line = 2.dp.toPx()
    drawLine(color, start, end, strokeWidth = line, pathEffect = PathEffect.dashPathEffect(floatArrayOf(6.dp.toPx(), 4.dp.toPx())))
    // the head, short of the ball
    val direction = end - start
    val length = direction.getDistance().takeIf { it > 0f } ?: return@Canvas
    val unit = direction / length
    val tip = end - unit * 12.dp.toPx()
    val normal = Offset(-unit.y, unit.x)
    val head = 7.dp.toPx()
    drawLine(color, tip, tip - unit * head + normal * head, strokeWidth = line, cap = StrokeCap.Round)
    drawLine(color, tip, tip - unit * head - normal * head, strokeWidth = line, cap = StrokeCap.Round)
}
