package io.github.naighes.scout4all

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.drawText
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import io.github.naighes.scout4all.core.CourtZoneEnum
import io.github.naighes.scout4all.core.FundamentalDetail
import io.github.naighes.scout4all.core.ServePathStats
import io.github.naighes.scout4all.core.ServeStartEnum
import kotlin.math.hypot
import kotlin.math.roundToInt

/** The court is 9 by 18 meters; a margin around it, and under our end line
 * the zones our serve starts from (as on the desktop). */
private const val Width = 9f
private const val Length = 18f
private const val Margin = 0.5f
private const val Gap = 0.4f
private const val Strip = 1.8f
private const val Across = Width + 2 * Margin
private const val Down = Length + Gap + Strip + 2 * Margin

/** Where the court is drawn: a meter, in pixels, from the top left corner
 * of the margin (the chart keeps its proportions). */
private class Place(val meter: Float) {
    fun at(x: Float, y: Float) = Offset((Margin + x) * meter, (Margin + y) * meter)
    fun start(i: Int, of: Int): Rect {
        val width = Width / of.coerceAtLeast(1)
        return Rect(at(i * width, Length + Gap), Size(width * meter, Strip * meter))
    }
}

/** Where our serves went from where they started, on the whole court:
 * their half at the top as our server sees it, ours below, the zones of our
 * end line under it with their errors; an arrow from each start to each
 * zone of their court, the more the thicker, in the color of their
 * efficiency (grey on too few to tell); their zones lit by how many went
 * there. A zone of our end line touched narrows to its serves, an arrow to
 * its serves (and its actions to watch), told under the court; again, all.
 * TalkBack reads the arrows, the most first. */
@Composable
internal fun ServePaths(
    f: FundamentalDetail,
    start: ServeStartEnum?,
    picked: Pair<ServeStartEnum, CourtZoneEnum>?,
    onStart: (ServeStartEnum) -> Unit,
    /** the arrow touched, and whether it was the one shown already (to show
     * all of them again) */
    onPath: (ServeStartEnum, CourtZoneEnum, Boolean) -> Unit,
) = Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
    Text(stringResource(R.string.serve_paths), Modifier.title(), style = MaterialTheme.typography.titleMedium)
    val both = f.servePaths.sumOf { it.count }
    Text(
        stringResource(R.string.serve_paths_told, count(both), f.total.toInt()),
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    // the arrow touched, while its start narrows the serves (the detail
    // loads again on the touch)
    var tapped by remember { mutableStateOf<Pair<ServeStartEnum, CourtZoneEnum>?>(null) }
    val told = (tapped?.takeIf { it.first == start } ?: picked)
        ?.let { (s, z) -> f.servePaths.firstOrNull { it.start == s && it.zone == z } }
    val colors = MaterialTheme.colorScheme
    val measurer = rememberTextMeasurer()
    val spoken = f.servePaths.joinToString("; ") { pathText(it) }
    // the whole court on the screen at once: at most two thirds of it high
    val high = LocalConfiguration.current.screenHeightDp.dp * 0.65f
    Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) {
        Canvas(
            Modifier.widthIn(max = high * (Across / Down)).fillMaxWidth().aspectRatio(Across / Down)
                .semantics { contentDescription = spoken }
                .pointerInput(f, start, picked) {
                    detectTapGestures { at ->
                        val place = Place(size.width / Across)
                        val zone = f.serveStarts.indices.firstOrNull { place.start(it, f.serveStarts.size).contains(at) }
                        val reach = 20.dp.toPx()
                        val path = f.servePaths
                            .map { it to distance(at, place.from(it), place.to(it)) }
                            .filter { it.second <= reach }
                            .minByOrNull { it.second }?.first
                        when {
                            zone != null -> {
                                tapped = null
                                onStart(f.serveStarts[zone].value)
                            }
                            path != null -> {
                                val touched = path.start to path.zone
                                val again = start == path.start && (tapped == touched || picked == touched)
                                tapped = touched.takeIf { !again }
                                onPath(path.start, path.zone, again)
                            }
                        }
                    }
                },
        ) {
            val place = Place(size.width / Across)
            drawCourt(place, f, colors.surfaceVariant, colors.onSurface)
            drawStarts(place, f, start, colors.surface, colors.onSurface, colors.primary, measurer)
            val most = f.servePaths.maxOfOrNull { it.count }?.coerceAtLeast(1.0) ?: 1.0
            f.servePaths.forEach { p ->
                val lit = told?.let { it.start == p.start && it.zone == p.zone } == true
                drawArrow(place, p, most, lit, colors.onSurfaceVariant, colors.onSurface)
            }
        }
    }
    Text(
        told?.let { pathText(it) } ?: stringResource(R.string.serve_paths_legend),
        style = if (told != null) MaterialTheme.typography.bodyLarge else MaterialTheme.typography.labelMedium,
        fontWeight = if (told != null) FontWeight.Bold else null,
        color = if (told != null) colors.onSurface else colors.onSurfaceVariant,
    )
}

/** "1 → 5 · 3 (40%) · +20%". */
private fun pathText(p: ServePathStats): String =
    listOfNotNull(
        p.label,
        "${count(p.count)} (${p.share.roundToInt()}%)",
        p.efficiency?.let { "%+d%%".format(it.roundToInt()) },
    ).joinToString(" · ")

private fun Place.from(p: ServePathStats) = at(p.fromX.toFloat(), Length + Gap)
private fun Place.to(p: ServePathStats) = at(p.toX.toFloat(), p.toY.toFloat())

/** How far `at` is from the segment from `a` to `b`. */
private fun distance(at: Offset, a: Offset, b: Offset): Float {
    val d = b - a
    val length = d.x * d.x + d.y * d.y
    val t = if (length > 0f) (((at.x - a.x) * d.x + (at.y - a.y) * d.y) / length).coerceIn(0f, 1f) else 0f
    val near = a + d * t
    return hypot(at.x - near.x, at.y - near.y)
}

private fun DrawScope.drawCourt(place: Place, f: FundamentalDetail, floor: Color, line: Color) {
    val court = Rect(place.at(0f, 0f), Size(Width * place.meter, Length * place.meter))
    drawRect(floor, court.topLeft, court.size)
    val most = f.targets.flatten().maxOfOrNull { it.count }?.coerceAtLeast(1.0) ?: 1.0
    f.targets.forEachIndexed { r, row ->
        row.forEachIndexed { c, t ->
            if (t.count > 0.0) drawRect(
                densityColor(t.count / most).copy(alpha = 0.55f),
                place.at(c * 3f, r * 3f),
                Size(3f * place.meter, 3f * place.meter),
            )
        }
    }
    val thin = line.copy(alpha = 0.3f)
    val width = 2.dp.toPx()
    for (i in 1..2) {
        drawLine(thin, place.at(i * 3f, 0f), place.at(i * 3f, 9f), strokeWidth = 1.dp.toPx())
        drawLine(thin, place.at(0f, i * 3f), place.at(Width, i * 3f), strokeWidth = 1.dp.toPx())
    }
    drawRect(line.copy(alpha = 0.7f), court.topLeft, court.size, style = Stroke(width))
    for (y in listOf(6f, 12f)) drawLine(line.copy(alpha = 0.7f), place.at(0f, y), place.at(Width, y), strokeWidth = width)
    drawLine(line, place.at(-Margin / 2, 9f), place.at(Width + Margin / 2, 9f), strokeWidth = 4.dp.toPx())
}

private fun DrawScope.drawStarts(
    place: Place,
    f: FundamentalDetail,
    picked: ServeStartEnum?,
    floor: Color,
    text: Color,
    accent: Color,
    measurer: androidx.compose.ui.text.TextMeasurer,
) {
    f.serveStarts.forEachIndexed { i, s ->
        val r = place.start(i, f.serveStarts.size).deflate(1.5.dp.toPx())
        drawRect(if (picked == s.value) accent.copy(alpha = 0.6f) else floor, r.topLeft, r.size)
        drawRect(text.copy(alpha = 0.3f), r.topLeft, r.size, style = Stroke(1.dp.toPx()))
        val lines = listOfNotNull(s.label, "= ${s.errors}".takeIf { s.errors > 0u })
        val style = TextStyle(
            color = text,
            fontSize = (place.meter * 0.5f).coerceAtMost(16.sp.toPx()).toSp(),
            fontWeight = FontWeight.Bold,
            textAlign = TextAlign.Center,
        )
        lines.forEachIndexed { n, line ->
            val color = if (n == 0) text else gradeColor(0.0)
            val measured = measurer.measure(line, style.copy(color = color, fontWeight = if (n == 0) FontWeight.Bold else null))
            val y = r.top + r.height * (if (lines.size == 1) 0.5f else 0.3f + 0.42f * n) - measured.size.height / 2
            drawText(measured, topLeft = Offset(r.center.x - measured.size.width / 2, y))
        }
    }
}

private fun DrawScope.drawArrow(place: Place, p: ServePathStats, most: Double, lit: Boolean, grey: Color, halo: Color) {
    val from = place.from(p)
    val to = place.to(p)
    val width = (0.06f + 0.26f * (p.count / most).toFloat()) * place.meter
    val color = p.efficiency?.let { gradeColor((it + 100.0) / 200.0) } ?: grey
    val d = to - from
    val length = hypot(d.x, d.y).coerceAtLeast(1f)
    val u = Offset(d.x / length, d.y / length)
    val head = (width * 2.2f).coerceAtLeast(8.dp.toPx())
    val base = to - u * head
    if (lit) drawLine(halo, from, to, strokeWidth = width + 4.dp.toPx())
    drawLine(color, from, base, strokeWidth = width)
    val half = head * 0.6f
    drawPath(
        Path().apply {
            moveTo(to.x, to.y)
            lineTo(base.x - u.y * half, base.y + u.x * half)
            lineTo(base.x + u.y * half, base.y - u.x * half)
            close()
        },
        color,
    )
}
