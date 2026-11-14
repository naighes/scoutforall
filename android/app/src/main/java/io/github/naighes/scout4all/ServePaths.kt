package io.github.naighes.scout4all

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
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
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.text.TextMeasurer
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.TextUnit
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
private const val Strip = 2.6f
/** How far into our court the stroke of the errors goes, in meters. */
private const val ErrorLength = 2f
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
 * end line under it with their errors (a red stroke ending in a cross);
 * an arrow from each start to each
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
            drawErrors(place, f, most, colors.surface, measurer)
            drawNumbers(place, f, colors.surface, colors.onSurface, measurer)
        }
    }
    if (told == null) PathsLegend()
    Text(
        told?.let { pathText(it) } ?: stringResource(R.string.serve_paths_legend),
        style = if (told != null) MaterialTheme.typography.bodyLarge else MaterialTheme.typography.labelMedium,
        fontWeight = if (told != null) FontWeight.Bold else null,
        color = if (told != null) colors.onSurface else colors.onSurfaceVariant,
    )
}

/** What the arrows tell: thicker the more, their color the efficiency,
 * the red cross the errors (as on the desktop). */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PathsLegend() = FlowRow(
    horizontalArrangement = Arrangement.spacedBy(16.dp),
    verticalArrangement = Arrangement.spacedBy(4.dp),
) {
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    val swatch = @Composable { width: Int, height: Int, color: Color ->
        Box(Modifier.size(width.dp, height.dp).background(color, RoundedCornerShape(2.dp)))
    }
    val item = @Composable { said: String, marks: @Composable () -> Unit ->
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(3.dp)) { marks() }
            Text(said, style = MaterialTheme.typography.labelMedium, color = muted)
        }
    }
    item(stringResource(R.string.paths_legend_more)) {
        swatch(18, 2, muted)
        swatch(18, 7, muted)
    }
    item(stringResource(R.string.paths_legend_color)) {
        listOf(0.0, 0.5, 1.0).forEach { swatch(14, 10, gradeColor(it)) }
        swatch(14, 10, muted)
    }
    item(stringResource(R.string.paths_legend_errors)) {
        Text("✕", color = gradeColor(0.0), fontWeight = FontWeight.Bold)
    }
}

/** The errors of each zone of our end line, which went nowhere on their
 * court: a red stroke up from it, sized as an arrow of as many serves,
 * ending in a cross, how many beside it. */
private fun DrawScope.drawErrors(place: Place, f: FundamentalDetail, most: Double, ink: Color, measurer: TextMeasurer) {
    val red = gradeColor(0.0)
    val size = (place.meter * 0.42f).coerceAtMost(15.sp.toPx()).toSp()
    val pad = 3.dp.toPx()
    f.serveStarts.forEachIndexed { i, s ->
        if (s.errors == 0u) return@forEachIndexed
        val r = place.start(i, f.serveStarts.size)
        val share = (s.errors.toDouble() / most).coerceAtMost(1.0).toFloat()
        val width = ((0.06f + 0.26f * share) * place.meter).coerceAtLeast(3.dp.toPx())
        // a little left of the middle, the arrows of the zone start there
        val x = r.left + r.width * 0.3f
        val from = Offset(x, r.top)
        val to = Offset(x, place.at(0f, Length - ErrorLength).y)
        drawLine(red.copy(alpha = 0.9f), from, to, strokeWidth = width)
        val arm = (width * 1.2f).coerceAtLeast(6.dp.toPx())
        drawLine(red, Offset(to.x - arm, to.y - arm), Offset(to.x + arm, to.y + arm), strokeWidth = 3.dp.toPx())
        drawLine(red, Offset(to.x + arm, to.y - arm), Offset(to.x - arm, to.y + arm), strokeWidth = 3.dp.toPx())
        val label = measurer.measure(s.errors.toString(), TextStyle(color = ink, fontSize = size, fontWeight = FontWeight.Bold))
        val topLeft = Offset(to.x + arm + pad, to.y - label.size.height / 2 - pad)
        drawRoundRect(red, topLeft, Size(label.size.width + 2 * pad, label.size.height + 2 * pad), CornerRadius(pad))
        drawText(label, topLeft = Offset(topLeft.x + pad, topLeft.y + pad))
    }
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
    measurer: TextMeasurer,
) {
    f.serveStarts.forEachIndexed { i, s ->
        val r = place.start(i, f.serveStarts.size).deflate(1.5.dp.toPx())
        drawRect(if (picked == s.value) accent.copy(alpha = 0.6f) else floor, r.topLeft, r.size)
        drawRect(text.copy(alpha = 0.3f), r.topLeft, r.size, style = Stroke(1.dp.toPx()))
        // its name; how many, their efficiency, the errors (which went
        // nowhere on their court), when there are; sized by the court, not
        // by the font of the phone, to stay in their zones
        val size = (place.meter * 0.5f).coerceAtMost(16.sp.toPx()).toSp()
        val small = (place.meter * 0.39f).coerceAtMost(12.5f.sp.toPx()).toSp()
        val lines = listOfNotNull(
            line(s.label, text, size, true),
            line(s.count.toString(), text, small, false).takeIf { s.count > 0u },
            s.efficiency?.let { line("%+d%%".format(it.roundToInt()), efficiencyColor(it), small, true) },
            line("✕ ${s.errors}", gradeColor(0.0), small, true).takeIf { s.errors > 0u },
        )
        lines.forEachIndexed { n, (content, style) ->
            val measured = measurer.measure(content, style)
            val y = r.top + r.height * (0.15f + 0.23f * n) - measured.size.height / 2
            drawText(measured, topLeft = Offset(r.center.x - measured.size.width / 2, y))
        }
    }
}

/** A line of text drawn on the court and its style. */
private fun line(text: String, color: Color, size: TextUnit, bold: Boolean) =
    text to TextStyle(color = color, fontSize = size, fontWeight = if (bold) FontWeight.Bold else null, textAlign = TextAlign.Center)

private fun efficiencyColor(efficiency: Double) = gradeColor((efficiency + 100.0) / 200.0)

/** In each zone of their court served to, at its top right corner over the
 * arrows: how many went there and, from a few on, their efficiency. */
private fun DrawScope.drawNumbers(place: Place, f: FundamentalDetail, panel: Color, text: Color, measurer: TextMeasurer) {
    val size = (place.meter * 0.42f).coerceAtMost(15.sp.toPx()).toSp()
    val pad = 3.dp.toPx()
    f.targets.forEachIndexed { r, row ->
        row.forEachIndexed { c, t ->
            if (t.count > 0.0) {
                val lines = listOfNotNull(
                    line(count(t.count), text, size, true),
                    t.efficiency?.let { line("%+d%%".format(it.roundToInt()), efficiencyColor(it), size, true) },
                ).map { (content, style) -> measurer.measure(content, style) }
                val width = lines.maxOf { it.size.width } + 2 * pad
                val height = lines.sumOf { it.size.height } + 2 * pad
                val corner = place.at(c * 3f + 3f, r * 3f)
                val topLeft = Offset(corner.x - width - pad, corner.y + pad)
                drawRoundRect(panel.copy(alpha = 0.85f), topLeft, Size(width, height), CornerRadius(pad))
                var y = topLeft.y + pad
                lines.forEach {
                    drawText(it, topLeft = Offset(topLeft.x + width - pad - it.size.width, y))
                    y += it.size.height
                }
            }
        }
    }
}

private fun DrawScope.drawArrow(place: Place, p: ServePathStats, most: Double, lit: Boolean, grey: Color, halo: Color) {
    val from = place.from(p)
    val to = place.to(p)
    val width = (0.06f + 0.26f * (p.count / most).toFloat()) * place.meter
    val color = p.efficiency?.let { efficiencyColor(it) } ?: grey
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
