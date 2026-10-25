package io.github.naighes.scout4all

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.animation.core.animateDpAsState
import androidx.compose.animation.core.tween
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.width
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
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import kotlin.math.ceil
import kotlin.math.roundToInt
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.zIndex
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.Hyphens
import androidx.compose.ui.text.style.LineBreak
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.unit.TextUnit
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
    /** above the others (a receiver above the players hiding behind):
     * where they overlap, a touch goes to it */
    val above: Boolean = false,
    /** the last touch of the rally: the symbol of its evaluation, empty
     * without one (a freeball); its word, for TalkBack */
    val touch: String? = null,
    val touchName: String? = null,
    /** plays the fundamental worth watching now: the efficiency in it over
     * the match, none before the first */
    val spotlight: Boolean = false,
    val efficiency: Double? = null,
    /** how good the efficiency is for the level of the team, 0 (red) to 1
     * (green); none without the level */
    val grade: Double? = null,
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
 * deep, to leave room for the choices below: as wide as it can, narrower
 * if not as high. With `serveZone`, the zone behind the end line, where the
 * server goes to serve. */
@Composable
fun Court(
    slot: (Int) -> CourtSlot,
    onPosition: (Int) -> Unit,
    modifier: Modifier = Modifier,
    showPositions: Boolean = false,
    serveZone: Boolean = false,
    ball: CourtBall? = null,
) = BoxWithConstraints(modifier.fillMaxWidth().padding(horizontal = 16.dp), contentAlignment = Alignment.TopCenter) {
    val zone = if (serveZone) ServeZone else 0.dp
    val width = when {
        constraints.hasBoundedHeight -> minOf(maxWidth, (maxHeight - zone).coerceAtLeast(0.dp) * 1.4f)
        else -> maxWidth
    }
    CourtColumn(slot, onPosition, showPositions, serveZone, ball, Modifier.width(width))
}

@Composable
private fun CourtColumn(
    slot: (Int) -> CourtSlot,
    onPosition: (Int) -> Unit,
    showPositions: Boolean,
    serveZone: Boolean,
    ball: CourtBall?,
    modifier: Modifier,
) = Column(modifier) {
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
            // the player on the spot, the name under the jersey in two
            // lines, a third of the court wide; on a court narrower than a
            // phone's (360dp), the jersey smaller, so that the players don't
            // overlap
            val scale = (maxWidth / 360.dp).coerceAtMost(1f)
            val width = maxWidth / 3
            val token = 44.dp * scale
            // each line takes whole pixels
            val height = token + 2.dp + with(LocalDensity.current) { (ceil(nameStyle().lineHeight.toPx()) * 2).toDp() }
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
                    Spot(position, s, showPositions, scale, Modifier.zIndex(if (s.above) 1f else 0f).offset(x, y).size(width, height)) {
                        onPosition(position)
                    }
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
        Row(Modifier.fillMaxWidth().height(ServeZone)) {
            Box(Modifier.weight(2f))
            when (server) {
                null -> Box(Modifier.weight(1f))
                else -> Spot(1, server, showPosition = false, scale = 1f, Modifier.weight(1f).fillMaxSize()) { onPosition(1) }
            }
        }
    }
}

private val ServeZone = 72.dp

/** The player of a position, all of it to touch: the jersey at the top,
 * the name under it. TalkBack reads the number, the name, then what the
 * badges say. */
@Composable
private fun Spot(position: Int, slot: CourtSlot, showPosition: Boolean, scale: Float, modifier: Modifier, onClick: () -> Unit) =
    Box(modifier.clickable(enabled = slot.enabled, role = Role.Button, onClick = onClick)) {
        if (showPosition) {
            Text(
                position.toString(),
                modifier = Modifier.align(Alignment.TopStart).padding(6.dp).readAs(stringResource(R.string.position_spoken, position)),
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
            CourtName(slot.name.orEmpty())
            badges(slot).takeIf { it.isNotEmpty() }?.let { Box(Modifier.readAs(it)) }
        }
    }

/** What the badges of the jersey say, in words. */
@Composable
private fun badges(slot: CourtSlot): String = listOfNotNull(
    stringResource(R.string.setter).takeIf { slot.setter },
    stringResource(R.string.serving).takeIf { slot.serving },
    slot.touchName?.let { stringResource(R.string.last_touch, it) },
    slot.spotlight.takeIf { it }?.let {
        stringResource(R.string.efficiency_spoken, slot.efficiency?.let { "${it.roundToInt()}%" } ?: stringResource(R.string.no_value))
    },
).joinToString(", ")

/** The name under the jersey in two lines, or the surname alone (all but
 * the first word) if it doesn't fit, never a word broken: the jersey tells
 * the rest. A surname too long even so is hyphenated. */
@Composable
private fun CourtName(name: String) {
    var surname by remember(name) { mutableStateOf(false) }
    val text = if (surname) name.substringAfter(' ') else name
    Text(
        text,
        modifier = Modifier.padding(horizontal = 4.dp).let { if (text.isEmpty()) it.notRead() else it },
        style = nameStyle().copy(
            hyphens = if (surname) Hyphens.Auto else Hyphens.None,
            lineBreak = LineBreak.Paragraph,
        ),
        maxLines = 2,
        overflow = TextOverflow.Ellipsis,
        textAlign = TextAlign.Center,
        onTextLayout = { layout ->
            val broken = (0 until layout.lineCount - 1).any {
                val end = layout.getLineEnd(it)
                end in 1 until text.length && text[end - 1] != ' ' && text[end] != ' '
            }
            if (!surname && (layout.didOverflowHeight || layout.isLineEllipsized(layout.lineCount - 1) || broken)) {
                surname = true
            }
        },
    )
}

/** The names on the court grow with the system font size up to 1.3 times,
 * not more: there's no room for them; the choices below show them whole. */
@Composable
private fun nameStyle(): TextStyle {
    val label = MaterialTheme.typography.labelSmall
    val scale = LocalDensity.current.fontScale.coerceAtMost(1.3f)
    return label.copy(
        fontSize = (label.fontSize.value * scale).dp.jerseySp(),
        lineHeight = (label.lineHeight.value * scale).dp.jerseySp(),
    )
}

/** A text of a jersey, drawn at its size: the system font size doesn't
 * make it larger than the jersey. */
@Composable
fun Dp.jerseySp(): TextUnit = with(LocalDensity.current) { toSp() }

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
            val size = (22.dp * scale).jerseySp()
            Text(
                slot.number?.toString() ?: "+",
                modifier = if (empty) Modifier.readAs(stringResource(R.string.no_player)) else Modifier,
                style = MaterialTheme.typography.titleLarge.copy(fontSize = size, lineHeight = size),
                fontWeight = FontWeight.Bold,
                color = number,
            )
        }
    }
    // TalkBack reads them after the name (`badges`)
    if (slot.setter) Badge(stringResource(R.string.setter_short), Modifier.align(Alignment.TopEnd).notRead())
    if (slot.spotlight) Efficiency(slot.efficiency, slot.grade, Modifier.align(Alignment.TopStart).offset(x = (-14).dp, y = (-6).dp).notRead())
    if (!slot.touch.isNullOrEmpty()) Badge(slot.touch, Modifier.align(Alignment.BottomStart).offset(x = (-4).dp, y = 4.dp).notRead())
    if (slot.serving) {
        Icon(
            painterResource(R.drawable.ic_ball),
            contentDescription = null,
            modifier = Modifier.align(Alignment.BottomStart).offset(x = (-14).dp, y = 4.dp).size(22.dp),
            tint = MaterialTheme.colorScheme.primary,
        )
    }
}

/** The efficiency of a player, on the jersey: from red to green for the
 * level of the team, grey without it; a dash before the first. */
@Composable
private fun Efficiency(efficiency: Double?, grade: Double?, modifier: Modifier) = Surface(
    modifier = modifier,
    shape = CircleShape,
    color = grade?.let { gradeColor(it) } ?: MaterialTheme.colorScheme.surfaceVariant,
    contentColor = grade?.let { Color.Black } ?: MaterialTheme.colorScheme.onSurfaceVariant,
) {
    val size = 11.dp.jerseySp()
    Text(
        efficiency?.let { "${it.roundToInt()}%" } ?: "–",
        modifier = Modifier.padding(horizontal = 5.dp, vertical = 1.dp),
        style = MaterialTheme.typography.labelSmall.copy(fontSize = size, lineHeight = size),
        fontWeight = FontWeight.Bold,
    )
}

/** On the jersey: the setter, the evaluation of the last touch. */
@Composable
private fun Badge(text: String, modifier: Modifier) = Surface(
    modifier = modifier.size(18.dp),
    shape = CircleShape,
    color = MaterialTheme.colorScheme.primary,
) {
    Box(contentAlignment = Alignment.Center) {
        val size = 11.dp.jerseySp()
        Text(
            text,
            style = MaterialTheme.typography.labelSmall.copy(fontSize = size, lineHeight = size),
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
