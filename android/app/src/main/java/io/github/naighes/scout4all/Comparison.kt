package io.github.naighes.scout4all

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.FundamentalStats
import io.github.naighes.scout4all.core.LiveStats
import io.github.naighes.scout4all.core.PhaseStats

/** How wide the name of a row stands between the two sides. */
private val LABEL_WIDTH = 72.dp

/** How thick a bar is. */
private val BAR_HEIGHT = 8.dp

/** The two sides of a match scouted on both sides, A | B, the same figures
 * as each side has (and as the comparison of the desktop): the fundamentals,
 * the phases, the errors, the distribution and the rotations. */
@Composable
fun Comparison(a: LiveStats, b: LiveStats) = Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(a.teamName, Modifier.weight(1f), style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold, color = TeamUs, textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis)
        Spacer(LABEL_WIDTH)
        Text(b.teamName, Modifier.weight(1f), style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold, color = TeamThem, textAlign = TextAlign.Center, maxLines = 2, overflow = TextOverflow.Ellipsis)
    }
    a.fundamentals.zip(b.fundamentals).forEach { (fa, fb) ->
        PercentRow(fa.name.replaceFirstChar { it.uppercase() }, fundamentalCell(fa), fundamentalCell(fb))
    }
    PercentRow(stringResourceOf(R.string.side_out), phaseCell(a.sideOut), phaseCell(b.sideOut))
    PercentRow(stringResourceOf(R.string.break_points), phaseCell(a.breakPoints), phaseCell(b.breakPoints))
    Section(R.string.stats_errors)
    listOf(
        Triple(R.string.unforced_errors, a.unforcedErrors, b.unforcedErrors),
        Triple(R.string.forced_errors, a.forcedErrors, b.forcedErrors),
        Triple(R.string.aces, a.aces, b.aces),
        Triple(R.string.opponent_errors, a.opponentErrors, b.opponentErrors),
    ).forEach { (name, x, y) ->
        // each row has its own scale: the longer side fills the bar
        val most = maxOf(x, y, 1u).toDouble()
        Line(
            stringResourceOf(name),
            { Cell(x.toString(), null, x.toDouble(), most, TeamUs, leftward = true) },
            { Cell(y.toString(), null, y.toDouble(), most, TeamThem, leftward = false) },
        )
    }
    Section(R.string.stats_distribution)
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Zones(a, Modifier.weight(1f))
        Spacer(LABEL_WIDTH)
        Zones(b, Modifier.weight(1f))
    }
    Section(R.string.rotations)
    a.rotations.forEach { ra ->
        val rb = b.rotations.firstOrNull { it.rotation == ra.rotation }?.total
        PercentRow(ra.name, phaseCell(ra.total), phaseCell(rb))
    }
}

/** What a side of a percentage row shows: the figure, its grade and the
 * counts under it. */
private data class Figure(val value: Double?, val grade: Double?, val counts: String)

private fun fundamentalCell(f: FundamentalStats) =
    Figure(f.efficiency, f.grade, "${f.total}·#${f.perfect}·=${f.errors}")

private fun phaseCell(p: PhaseStats?) = Figure(p?.percentage, p?.grade, p?.let { "${it.won}/${it.rallies}" }.orEmpty())

@Composable
private fun stringResourceOf(id: Int) = androidx.compose.ui.res.stringResource(id)

@Composable
private fun Spacer(width: androidx.compose.ui.unit.Dp) = Box(Modifier.width(width))

@Composable
private fun Section(title: Int) = Text(
    stringResourceOf(title),
    Modifier.title().padding(top = 4.dp),
    style = MaterialTheme.typography.titleMedium,
)

@Composable
private fun PercentRow(name: String, a: Figure, b: Figure) = Line(
    name,
    { Cell(pct(a.value), a.grade, a.value ?: 0.0, 100.0, TeamUs, leftward = true, small = a.counts) },
    { Cell(pct(b.value), b.grade, b.value ?: 0.0, 100.0, TeamThem, leftward = false, small = b.counts) },
)

/** One row: the side A, the name, the side B. */
@Composable
private fun Line(name: String, a: @Composable () -> Unit, b: @Composable () -> Unit) = Row(
    Modifier.fillMaxWidth().semantics(mergeDescendants = true) {},
    horizontalArrangement = Arrangement.spacedBy(8.dp),
    verticalAlignment = Alignment.CenterVertically,
) {
    Box(Modifier.weight(1f)) { a() }
    Text(
        name,
        Modifier.width(LABEL_WIDTH),
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        textAlign = TextAlign.Center,
    )
    Box(Modifier.weight(1f)) { b() }
}

/** One side of a row: the figure outside, the bar growing from the name
 * toward it, the counts under both. */
@Composable
private fun Cell(
    figure: String,
    grade: Double?,
    share: Double,
    of: Double,
    color: Color,
    leftward: Boolean,
    small: String = "",
) = Column(Modifier.fillMaxWidth(), horizontalAlignment = if (leftward) Alignment.Start else Alignment.End) {
    val number = @Composable {
        Text(
            figure,
            Modifier.width(40.dp),
            style = MaterialTheme.typography.titleSmall,
            fontWeight = FontWeight.Bold,
            color = grade?.let { gradeColor(it) } ?: MaterialTheme.colorScheme.onSurface,
            textAlign = if (leftward) TextAlign.Start else TextAlign.End,
        )
    }
    val bar = @Composable {
        Box(
            Modifier.weight(1f).height(BAR_HEIGHT).clip(RoundedCornerShape(2.dp))
                .background(MaterialTheme.colorScheme.outlineVariant),
            contentAlignment = if (leftward) Alignment.CenterEnd else Alignment.CenterStart,
        ) {
            Box(Modifier.fillMaxWidth((share.coerceIn(0.0, of) / of).toFloat()).height(BAR_HEIGHT).background(color))
        }
    }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        if (leftward) { number(); bar() } else { bar(); number() }
    }
    if (small.isNotEmpty()) {
        Text(small, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
    }
}

/** The zones of the setter on the court, the share of each. */
@Composable
private fun Zones(stats: LiveStats, modifier: Modifier) = Column(modifier, verticalArrangement = Arrangement.spacedBy(4.dp)) {
    listOf(listOf(4, 3, 2), listOf(0, 8, 9)).forEach { row ->
        Row(Modifier.fillMaxWidth()) {
            row.forEach { zone ->
                Column(Modifier.weight(1f), horizontalAlignment = Alignment.CenterHorizontally) {
                    if (zone != 0) {
                        Text(zone.toString(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        Text(
                            stats.distribution.firstOrNull { it.zone.toInt() == zone }?.let { "${Math.round(it.share)}%" } ?: "–",
                            style = MaterialTheme.typography.titleSmall,
                            fontWeight = FontWeight.Bold,
                        )
                    }
                }
            }
        }
    }
}
