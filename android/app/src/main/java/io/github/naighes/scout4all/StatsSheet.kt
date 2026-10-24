package io.github.naighes.scout4all

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.pager.HorizontalPager
import androidx.compose.foundation.pager.rememberPagerState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.AttackStats
import io.github.naighes.scout4all.core.ErrorCount
import io.github.naighes.scout4all.core.EventTypeEnum
import io.github.naighes.scout4all.core.FundamentalStats
import io.github.naighes.scout4all.core.LiveStats
import io.github.naighes.scout4all.core.PhaseStats
import io.github.naighes.scout4all.core.StatsScope
import io.github.naighes.scout4all.core.TrendSample
import io.github.naighes.scout4all.core.ZoneStats
import kotlin.math.roundToInt
import kotlinx.coroutines.launch

/** How good a value is: red at `bad` or below, yellow halfway, green at
 * `good` or above. */
fun scaleColor(value: Double, bad: Double, good: Double): Color {
    val t = ((value - bad) / (good - bad)).coerceIn(0.0, 1.0).toFloat()
    return when {
        t < 0.5f -> lerp(Lost, Fair, t * 2)
        else -> lerp(Fair, Won, (t - 0.5f) * 2)
    }
}

/** Where each figure turns red and green: efficiencies differ by
 * fundamental (a serve putting them in trouble is already good). To tune
 * on real matches. */
private fun efficiencyScale(event: EventTypeEnum): Pair<Double, Double> = when (event) {
    EventTypeEnum.P -> 0.0 to 50.0
    EventTypeEnum.A -> 0.0 to 40.0
    EventTypeEnum.S -> 0.0 to 30.0
    else -> 0.0 to 40.0
}
private val sideOutScale = 40.0 to 65.0
private val breakScale = 25.0 to 50.0

private fun pct(value: Double?) = value?.let { "${it.roundToInt()}%" } ?: "–"

/** Under the choices, where the thumb is: touch it or swipe it up for the
 * statistics. */
@Composable
fun StatsHandle(onOpen: () -> Unit, modifier: Modifier = Modifier) = Column(
    modifier.clip(RoundedCornerShape(50)).clickable(onClick = onOpen).swipeUp(onOpen)
        .padding(horizontal = 16.dp, vertical = 4.dp),
    horizontalAlignment = Alignment.CenterHorizontally,
) {
    Icon(Icons.Default.KeyboardArrowUp, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    Text(
        stringResource(R.string.stats_swipe),
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** Calls `onSwipe` when dragged up far enough. */
@Composable
fun Modifier.swipeUp(onSwipe: () -> Unit): Modifier {
    var dragged by remember { mutableFloatStateOf(0f) }
    val threshold = with(LocalDensity.current) { 32.dp.toPx() }
    return draggable(
        rememberDraggableState { dragged += it },
        Orientation.Vertical,
        onDragStarted = { dragged = 0f },
        onDragStopped = { if (dragged < -threshold) onSwipe() },
    )
}

private enum class StatsPage(val title: Int) {
    Summary(R.string.stats_summary),
    Distribution(R.string.stats_distribution),
    Errors(R.string.stats_errors),
    Trend(R.string.stats_trend),
}

/** The statistics while scouting, from below: the set being played at
 * first, any other set or the whole match on a chip; the essentials first,
 * the details a swipe aside. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun StatsSheet(
    stats: LiveStats?,
    currentSet: Int,
    onScope: (StatsScope) -> Unit,
    onDismiss: () -> Unit,
) {
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    LaunchedEffect(Unit) { onScope(StatsScope.Set(currentSet.toUByte())) }
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheet) {
        Column(Modifier.fillMaxWidth().fillMaxHeight(0.9f)) {
            Scopes(stats, currentSet, onScope)
            when (stats) {
                null -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
                else -> Pages(stats)
            }
        }
    }
}

/** A chip for each set so far, then the match. */
@Composable
private fun Scopes(stats: LiveStats?, currentSet: Int, onScope: (StatsScope) -> Unit) = Row(
    Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp),
    horizontalArrangement = Arrangement.spacedBy(8.dp),
) {
    val sets = stats?.sets?.map { it.toInt() } ?: listOf(currentSet)
    val scope = stats?.scope
    sets.forEach { n ->
        FilterChip(
            selected = scope is StatsScope.Set && scope.number.toInt() == n,
            onClick = { onScope(StatsScope.Set(n.toUByte())) },
            label = { Text(stringResource(R.string.set_n, n)) },
        )
    }
    FilterChip(
        selected = scope is StatsScope.Match,
        onClick = { onScope(StatsScope.Match) },
        label = { Text(stringResource(R.string.whole_match)) },
    )
}

@Composable
private fun Pages(stats: LiveStats) {
    val pages = StatsPage.entries
    val pager = rememberPagerState(pageCount = { pages.size })
    val scope = rememberCoroutineScope()
    PrimaryTabRow(selectedTabIndex = pager.currentPage) {
        pages.forEachIndexed { i, page ->
            Tab(
                selected = pager.currentPage == i,
                onClick = { scope.launch { pager.animateScrollToPage(i) } },
                text = { Text(stringResource(page.title), maxLines = 1, overflow = TextOverflow.Ellipsis) },
            )
        }
    }
    HorizontalPager(pager, Modifier.fillMaxSize(), verticalAlignment = Alignment.Top) { i ->
        Column(
            Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            when (pages[i]) {
                StatsPage.Summary -> Summary(stats)
                StatsPage.Distribution -> Distribution(stats)
                StatsPage.Errors -> Errors(stats)
                StatsPage.Trend -> Trend(stats)
            }
        }
    }
}

// --- summary ------------------------------------------------------------

/** What the coach looks at first: the phases, the efficiency of each
 * fundamental, the unforced errors. */
@Composable
private fun Summary(stats: LiveStats) {
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        PhaseTile(stringResource(R.string.side_out), stats.sideOut, sideOutScale, Modifier.weight(1f))
        PhaseTile(stringResource(R.string.break_points), stats.breakPoints, breakScale, Modifier.weight(1f))
    }
    val fundamentals = stats.fundamentals.filter { it.event != EventTypeEnum.D }
    fundamentals.chunked(2).forEach { row ->
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            row.forEach { FundamentalTile(it, Modifier.weight(1f)) }
            repeat(2 - row.size) { Spacer(Modifier.weight(1f)) }
        }
    }
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        CountTile(
            stringResource(R.string.unforced_errors),
            stats.unforcedErrors.toString(),
            stringResource(R.string.forced_errors) + " " + stats.forcedErrors,
            Modifier.weight(1f),
            color = if (stats.unforcedErrors > 0u) Lost else null,
        )
        CountTile(
            stringResource(R.string.opponent_errors),
            stats.opponentErrors.toString(),
            stringResource(R.string.opponent_points) + " " + stats.opponentPoints,
            Modifier.weight(1f),
        )
    }
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        CountTile(
            stringResource(R.string.aces),
            "${stats.aces} – ${stats.acesAgainst}",
            stringResource(R.string.ours) + " – " + stringResource(R.string.theirs),
            Modifier.weight(1f),
        )
        Spacer(Modifier.weight(1f))
    }
}

@Composable
private fun Tile(modifier: Modifier, content: @Composable () -> Unit) = Surface(
    modifier,
    shape = MaterialTheme.shapes.medium,
    color = MaterialTheme.colorScheme.surfaceVariant,
) {
    Column(Modifier.padding(12.dp)) { content() }
}

@Composable
private fun TileLabel(text: String) = Text(
    text,
    style = MaterialTheme.typography.labelMedium,
    color = MaterialTheme.colorScheme.onSurfaceVariant,
    maxLines = 1,
    overflow = TextOverflow.Ellipsis,
)

@Composable
private fun TileValue(text: String, color: Color?) = Text(
    text,
    style = MaterialTheme.typography.headlineMedium,
    fontWeight = FontWeight.Bold,
    color = color ?: MaterialTheme.colorScheme.onSurface,
)

@Composable
private fun TileDetail(text: String) = Text(
    text,
    style = MaterialTheme.typography.labelSmall,
    color = MaterialTheme.colorScheme.onSurfaceVariant,
    maxLines = 1,
    overflow = TextOverflow.Ellipsis,
)

@Composable
private fun PhaseTile(label: String, phase: PhaseStats?, scale: Pair<Double, Double>, modifier: Modifier) =
    Tile(modifier) {
        TileLabel(label)
        TileValue(pct(phase?.percentage), phase?.let { scaleColor(it.percentage, scale.first, scale.second) })
        TileDetail(phase?.let { stringResource(R.string.rallies_detail, it.won.toInt(), it.rallies.toInt()) } ?: "")
    }

@Composable
private fun FundamentalTile(f: FundamentalStats, modifier: Modifier) = Tile(modifier) {
    val (bad, good) = efficiencyScale(f.event)
    TileLabel(f.name.replaceFirstChar { it.uppercase() })
    TileValue(pct(f.efficiency), f.efficiency?.let { scaleColor(it, bad, good) })
    TileDetail(
        if (f.total > 0u) stringResource(R.string.touches_detail, f.total.toInt(), f.perfect.toInt(), f.errors.toInt())
        else "",
    )
}

@Composable
private fun CountTile(label: String, value: String, detail: String, modifier: Modifier, color: Color? = null) =
    Tile(modifier) {
        TileLabel(label)
        TileValue(value, color)
        TileDetail(detail)
    }

// --- distribution -------------------------------------------------------

private enum class AttackKind(val title: Int) {
    All(R.string.all_attacks),
    First(R.string.first_attack),
    Counter(R.string.counter_attack),
}

/** The distribution of the setter on half a court: the share of each
 * target, and how well we attack there. */
@Composable
private fun Distribution(stats: LiveStats) {
    var kind by remember { mutableIntStateOf(0) }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        AttackKind.entries.forEachIndexed { i, k ->
            FilterChip(selected = kind == i, onClick = { kind = i }, label = { Text(stringResource(k.title)) })
        }
    }
    val (zones, summary) = when (AttackKind.entries[kind]) {
        AttackKind.All -> stats.distribution to null
        AttackKind.First -> stats.firstAttackDistribution to stats.firstAttack
        AttackKind.Counter -> stats.counterAttackDistribution to stats.counterAttack
    }
    when {
        zones.isEmpty() -> Text(stringResource(R.string.no_attacks), color = MaterialTheme.colorScheme.onSurfaceVariant)
        else -> DistributionCourt(zones)
    }
    summary?.let { AttackLine(stringResource(AttackKind.entries[kind].title), it) }
    if (AttackKind.entries[kind] == AttackKind.All) {
        stats.firstAttack?.let { AttackLine(stringResource(R.string.first_attack), it) }
        stats.counterAttack?.let { AttackLine(stringResource(R.string.counter_attack), it) }
    }
}

@Composable
private fun AttackLine(label: String, attacks: AttackStats) = Row(verticalAlignment = Alignment.CenterVertically) {
    val (bad, good) = efficiencyScale(EventTypeEnum.A)
    Text(label, Modifier.weight(1f), style = MaterialTheme.typography.bodyMedium)
    Text(
        "${attacks.total} · #${attacks.kills} · = ${attacks.errors} · / ${attacks.blocked}",
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Text(
        pct(attacks.efficiency),
        Modifier.width(56.dp),
        style = MaterialTheme.typography.titleMedium,
        fontWeight = FontWeight.Bold,
        textAlign = TextAlign.End,
        color = scaleColor(attacks.efficiency, bad, good),
    )
}

/** Half a court, the net on top: 4 3 2 in the front row, 8 (pipe) and 9
 * (from 1) behind the attack line. */
@Composable
private fun DistributionCourt(zones: List<ZoneStats>) {
    val line = MaterialTheme.colorScheme.outline
    Box(
        Modifier.fillMaxWidth().aspectRatio(1.4f)
            .border(2.dp, line, RoundedCornerShape(2.dp)),
    ) {
        Canvas(Modifier.fillMaxSize()) {
            // the net, then the attack line halfway (not to scale)
            drawLine(line, Offset(0f, 0f), Offset(size.width, 0f), strokeWidth = 6.dp.toPx())
            drawLine(
                line,
                Offset(0f, size.height / 2),
                Offset(size.width, size.height / 2),
                strokeWidth = 1.dp.toPx(),
                pathEffect = PathEffect.dashPathEffect(floatArrayOf(12f, 8f)),
            )
        }
        Column(Modifier.fillMaxSize()) {
            listOf(listOf(4, 3, 2), listOf(null, 8, 9)).forEach { row ->
                Row(Modifier.weight(1f).fillMaxWidth()) {
                    row.forEach { zone ->
                        Box(Modifier.weight(1f).fillMaxHeight(), contentAlignment = Alignment.Center) {
                            zone?.let { z -> ZoneCell(z, zones.find { it.zone.toInt() == z }) }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ZoneCell(zone: Int, stats: ZoneStats?) = Column(horizontalAlignment = Alignment.CenterHorizontally) {
    val (bad, good) = efficiencyScale(EventTypeEnum.A)
    Text(zone.toString(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
    Text(
        pct(stats?.share),
        style = MaterialTheme.typography.headlineSmall,
        fontWeight = FontWeight.Bold,
        color = if (stats == null) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
    )
    stats?.let {
        Surface(
            shape = RoundedCornerShape(50),
            color = scaleColor(it.attacks.efficiency, bad, good),
            contentColor = Color.Black,
        ) {
            Text(
                pct(it.attacks.efficiency),
                Modifier.padding(horizontal = 8.dp),
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.Bold,
            )
        }
        Text(
            "${it.attacks.total} att",
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

// --- errors -------------------------------------------------------------

private val unforcedNames = mapOf(
    EventTypeEnum.S to R.string.error_serve,
    EventTypeEnum.A to R.string.error_attack,
    EventTypeEnum.B to R.string.error_block_invasion,
    EventTypeEnum.F to R.string.error_fault,
)
private val forcedNames = mapOf(
    EventTypeEnum.A to R.string.error_blocked,
    EventTypeEnum.B to R.string.error_block,
    EventTypeEnum.P to R.string.error_reception,
    EventTypeEnum.D to R.string.error_dig,
)

/** Our errors, the unforced ones first and in red, by fundamental; then
 * theirs and the aces. */
@Composable
private fun Errors(stats: LiveStats) {
    val most = (stats.unforcedByEvent + stats.forcedByEvent).maxOfOrNull { it.count.toInt() }?.coerceAtLeast(1) ?: 1
    ErrorGroup(stringResource(R.string.unforced_errors), stats.unforcedErrors.toInt(), stats.unforcedByEvent, unforcedNames, Lost, most)
    ErrorGroup(
        stringResource(R.string.forced_errors),
        stats.forcedErrors.toInt(),
        stats.forcedByEvent,
        forcedNames,
        MaterialTheme.colorScheme.onSurfaceVariant,
        most,
    )
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        CountTile(
            stringResource(R.string.opponent_errors),
            stats.opponentErrors.toString(),
            stringResource(R.string.opponent_points) + " " + stats.opponentPoints,
            Modifier.weight(1f),
        )
        CountTile(
            stringResource(R.string.aces),
            "${stats.aces} – ${stats.acesAgainst}",
            stringResource(R.string.ours) + " – " + stringResource(R.string.theirs),
            Modifier.weight(1f),
        )
    }
}

@Composable
private fun ErrorGroup(
    title: String,
    total: Int,
    errors: List<ErrorCount>,
    names: Map<EventTypeEnum, Int>,
    color: Color,
    most: Int,
) = Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(title, Modifier.weight(1f), style = MaterialTheme.typography.titleMedium)
        Text(total.toString(), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold, color = color)
    }
    errors.forEach { e ->
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                names[e.event]?.let { stringResource(it) } ?: e.event.name,
                Modifier.width(132.dp),
                style = MaterialTheme.typography.bodyMedium,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Box(Modifier.weight(1f).height(12.dp)) {
                Box(
                    Modifier.fillMaxHeight().fillMaxWidth(e.count.toFloat() / most)
                        .clip(RoundedCornerShape(50)).background(color),
                )
            }
            Text(
                e.count.toString(),
                Modifier.width(32.dp),
                textAlign = TextAlign.End,
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = FontWeight.Bold,
            )
        }
    }
}

// --- trend --------------------------------------------------------------

/** How the efficiencies and the phases went rally by rally: the value so
 * far after each one, a dashed line where a set starts. */
@Composable
private fun Trend(stats: LiveStats) {
    if (stats.trend.isEmpty()) {
        Text(stringResource(R.string.no_rallies), color = MaterialTheme.colorScheme.onSurfaceVariant)
        return
    }
    Text(stringResource(R.string.trend_note), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    val name = { e: EventTypeEnum ->
        stats.fundamentals.find { it.event == e }?.name?.replaceFirstChar { it.uppercase() } ?: e.name
    }
    val series = listOf(
        Triple(name(EventTypeEnum.P), efficiencyScale(EventTypeEnum.P)) { t: TrendSample -> t.reception },
        Triple(name(EventTypeEnum.A), efficiencyScale(EventTypeEnum.A)) { t: TrendSample -> t.attack },
        Triple(stringResource(R.string.trend_counter_attack), efficiencyScale(EventTypeEnum.A)) { t: TrendSample -> t.counterAttack },
        Triple(name(EventTypeEnum.S), efficiencyScale(EventTypeEnum.S)) { t: TrendSample -> t.serve },
        Triple(stringResource(R.string.side_out), sideOutScale) { t: TrendSample -> t.sideOut },
        Triple(stringResource(R.string.break_points), breakScale) { t: TrendSample -> t.breakPoints },
    )
    series.forEach { (label, scale, value) ->
        TrendChart(label, stats.trend, stats.trend.map(value), scale)
    }
}

@Composable
private fun TrendChart(label: String, samples: List<TrendSample>, values: List<Double?>, scale: Pair<Double, Double>) =
    Column {
        val last = values.lastOrNull { it != null }
        Row(verticalAlignment = Alignment.Bottom) {
            Text(label, Modifier.weight(1f), style = MaterialTheme.typography.titleSmall)
            Text(
                pct(last),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
                color = last?.let { scaleColor(it, scale.first, scale.second) } ?: MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        val lineColor = MaterialTheme.colorScheme.primary
        val axis = MaterialTheme.colorScheme.outline
        val known = values.filterNotNull()
        // always show 0 and the green mark, the line in between
        val low = minOf(0.0, known.minOrNull() ?: 0.0)
        val high = maxOf(scale.second, known.maxOrNull() ?: 0.0)
        Canvas(Modifier.fillMaxWidth().height(72.dp).padding(vertical = 4.dp)) {
            val step = if (values.size > 1) size.width / (values.size - 1) else 0f
            fun y(v: Double) = (size.height * (1 - (v - low) / (high - low))).toFloat()
            val dashed = PathEffect.dashPathEffect(floatArrayOf(8f, 8f))
            drawLine(axis, Offset(0f, y(0.0)), Offset(size.width, y(0.0)), strokeWidth = 1.dp.toPx())
            drawLine(Won.copy(alpha = 0.4f), Offset(0f, y(scale.second)), Offset(size.width, y(scale.second)), 1.dp.toPx(), pathEffect = dashed)
            // a set starts
            samples.zipWithNext().forEachIndexed { i, (a, b) ->
                if (a.setNumber != b.setNumber) {
                    val x = step * (i + 0.5f)
                    drawLine(axis, Offset(x, 0f), Offset(x, size.height), 1.dp.toPx(), pathEffect = dashed)
                }
            }
            val path = Path()
            var drawing = false
            values.forEachIndexed { i, v ->
                when {
                    v == null -> drawing = false
                    drawing -> path.lineTo(step * i, y(v))
                    else -> {
                        path.moveTo(step * i, y(v))
                        drawing = true
                    }
                }
            }
            drawPath(path, lineColor, style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round))
            values.withIndex().lastOrNull { it.value != null }?.let { (i, v) ->
                drawCircle(lineColor, 3.dp.toPx(), Offset(step * i, y(v ?: 0.0)))
            }
        }
    }
