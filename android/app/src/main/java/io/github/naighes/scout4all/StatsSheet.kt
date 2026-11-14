package io.github.naighes.scout4all

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.Orientation
import androidx.compose.foundation.gestures.draggable
import androidx.compose.foundation.gestures.rememberDraggableState
import androidx.compose.foundation.horizontalScroll
import androidx.compose.ui.text.rememberTextMeasurer
import androidx.compose.ui.unit.Dp
import androidx.compose.foundation.ScrollState
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.WindowInsetsSides
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.only
import androidx.compose.foundation.layout.safeDrawing
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
import androidx.compose.material3.PrimaryScrollableTabRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.lerp
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.AttackStats
import io.github.naighes.scout4all.core.DetailFilter
import io.github.naighes.scout4all.core.DetailStats
import io.github.naighes.scout4all.core.ErrorCount
import io.github.naighes.scout4all.core.EventTypeEnum
import io.github.naighes.scout4all.core.FundamentalStats
import io.github.naighes.scout4all.core.LiveStats
import io.github.naighes.scout4all.core.SetPoints
import io.github.naighes.scout4all.core.PhaseStats
import io.github.naighes.scout4all.core.PossessionStats
import io.github.naighes.scout4all.core.RotationStats
import io.github.naighes.scout4all.core.StatsScope
import io.github.naighes.scout4all.core.SituationChoices
import androidx.compose.ui.platform.LocalContext
import io.github.naighes.scout4all.core.TrendLine
import io.github.naighes.scout4all.core.ZoneStats
import kotlin.math.roundToInt
import kotlinx.coroutines.launch

/** How good a figure is for the level of the team (the grade of the
 * core): red at 0, yellow at 0.5, green at 1. */
fun gradeColor(grade: Double): Color {
    val t = grade.coerceIn(0.0, 1.0).toFloat()
    return when {
        t < 0.5f -> lerp(Lost, Fair, t * 2)
        else -> lerp(Fair, Won, (t - 0.5f) * 2)
    }
}

/** A color for each set in the trend: none of them amber (the actions),
 * red or green (the grades). */
internal val setColors = listOf(
    Color(0xFF60A5FA),
    Color(0xFFF472B6),
    Color(0xFFA78BFA),
    Color(0xFF2DD4BF),
    Color(0xFFFB923C),
)

private fun setColor(setNumber: Int) = setColors[(setNumber - 1).coerceIn(0, setColors.size - 1)]

internal fun pct(value: Double?) = value?.let { "${it.roundToInt()}%" } ?: "–"

/** Under the choices, where the thumb is: touch it or swipe it up for the
 * statistics. */
@Composable
fun StatsHandle(onOpen: () -> Unit, modifier: Modifier = Modifier) = Column(
    modifier.clip(RoundedCornerShape(50)).clickable(role = Role.Button, onClick = onOpen).swipeUp(onOpen)
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
    Detail(R.string.stats_detail),
}

/** Reads a fundamental of the scope in detail, off the main thread; none
 * when it can't. */
typealias LoadDetail = suspend (StatsScope, EventTypeEnum, DetailFilter) -> DetailStats?

/** The statistics while scouting, from below: the set being played at
 * first, any other set or the whole match on a chip; the essentials first,
 * the details a swipe aside. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun StatsSheet(
    stats: LiveStats?,
    currentSet: Int,
    onScope: (StatsScope) -> Unit,
    loadDetail: LoadDetail,
    onDismiss: () -> Unit,
) {
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    LaunchedEffect(Unit) { onScope(StatsScope.Set(currentSet.toUByte())) }
    // the step of the tutorial that opens it is over
    val tutorial = LocalTutorial.current
    LaunchedEffect(Unit) { tutorial?.reached(STATS_OPENED) }
    // only the bottom inset: the top one is padded by how far the sheet is
    // from the top, its height would move it and it would never stop
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheet,
        contentWindowInsets = { WindowInsets.safeDrawing.only(WindowInsetsSides.Bottom) },
    ) {
        Column(Modifier.fillMaxWidth().fillMaxHeight(0.9f)) {
            when (stats) {
                null -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
                else -> StatsPages(stats, onScope, loadDetail)
            }
        }
    }
}

/** A chip for each set so far, then the match. */
@Composable
private fun Scopes(stats: LiveStats, onScope: (StatsScope) -> Unit) = Row(
    Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp),
    horizontalArrangement = Arrangement.spacedBy(8.dp),
) {
    val scope = stats.scope
    stats.sets.map { it.toInt() }.forEach { n ->
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

/** The pages, a swipe aside; the scope on top, but on the trend, which
 * shows every set. While scouting and on the page of a match. */
@Composable
fun StatsPages(
    stats: LiveStats,
    onScope: (StatsScope) -> Unit,
    loadDetail: LoadDetail,
    // the actions of the detail in the video: not while scouting
    watch: ((StatsScope) -> Watch)? = null,
) {
    val pages = StatsPage.entries
    val pager = rememberPagerState(pageCount = { pages.size })
    val scope = rememberCoroutineScope()
    // the detail asked for stays while swiping, and the last one read while
    // the next is
    var query by remember { mutableStateOf(DetailQuery()) }
    val detail by produceState<DetailStats?>(null, stats.scope, query) {
        value = loadDetail(stats.scope, query.event, query.filter) ?: value
    }
    // the choices of the filters on the situation of the game
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val situation by produceState<SituationChoices?>(null) { value = app.core { it.situationChoices() } }
    if (pages[pager.currentPage] != StatsPage.Trend) Scopes(stats, onScope)
    PrimaryScrollableTabRow(selectedTabIndex = pager.currentPage, edgePadding = 0.dp) {
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
                StatsPage.Detail -> Detail(detail, query, situation, watch = watch?.invoke(stats.scope)) { query = it }
            }
        }
    }
}

// --- summary ------------------------------------------------------------

/** What the coach looks at first: the phases, the efficiency of each
 * fundamental, the unforced errors. */
@Composable
internal fun Summary(stats: LiveStats) {
    if (!stats.graded) {
        Text(
            stringResource(R.string.set_team_level),
            style = MaterialTheme.typography.labelMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        PhaseTile(stringResource(R.string.side_out), stats.sideOut, Modifier.weight(1f))
        PhaseTile(stringResource(R.string.break_points), stats.breakPoints, Modifier.weight(1f))
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
    Rotations(stats.rotations)
    Possessions(stats.sideOutPossessions)
}

/** The side out by possession: how often the first ball, the second and
 * the later ones end with our point. */
@Composable
private fun Possessions(possessions: List<PossessionStats>) = Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
    Text(stringResource(R.string.side_out_by_possession), Modifier.title(), style = MaterialTheme.typography.titleMedium)
    possessions.forEach { p ->
        Row(Modifier.semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.Bottom) {
            Text(p.name, Modifier.weight(1f), style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
            if (p.total > 0u) {
                Text(
                    stringResource(R.string.rallies_detail, p.won.toInt(), p.total.toInt()) + "  ",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Text(
                pct(p.percentage),
                style = MaterialTheme.typography.titleMedium,
                fontWeight = FontWeight.Bold,
            )
        }
    }
}

/** The side out and the break points of each rotation, side by side, then
 * all its rallies: the points won minus lost over the rallies won. */
@Composable
private fun Rotations(rotations: List<RotationStats>) = Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
    Text(stringResource(R.string.rotations), Modifier.title(), style = MaterialTheme.typography.titleMedium)
    Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
        Spacer(Modifier.width(32.dp))
        listOf(R.string.side_out, R.string.break_points, R.string.total).forEach {
            Text(
                stringResource(it),
                Modifier.weight(1f),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.End,
            )
        }
    }
    rotations.forEach { r ->
        Row(
            Modifier.semantics(mergeDescendants = true) {},
            horizontalArrangement = Arrangement.spacedBy(12.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(r.name, Modifier.width(32.dp), style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.Bold)
            listOf(r.sideOut, r.breakPoints).forEach { p ->
                RotationCell(Modifier.weight(1f), p) {
                    Row(verticalAlignment = Alignment.Bottom) {
                        p?.let {
                            Text(
                                signed(it.balance),
                                Modifier.padding(end = 8.dp),
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        Text(
                            pct(p?.percentage),
                            style = MaterialTheme.typography.titleMedium,
                            fontWeight = FontWeight.Bold,
                            color = p?.grade?.let { gradeColor(it) } ?: MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
            RotationCell(Modifier.weight(1f), r.total) {
                Text(
                    r.total?.let { signed(it.balance) } ?: "–",
                    style = MaterialTheme.typography.titleMedium,
                    fontWeight = FontWeight.Bold,
                    color = r.total?.balance?.let { b ->
                        when {
                            b > 0 -> Won
                            b < 0 -> Lost
                            else -> null
                        }
                    } ?: MaterialTheme.colorScheme.onSurface,
                )
            }
        }
    }
}

/** A phase of a rotation: its figure over the rallies won of the played ones. */
@Composable
private fun RotationCell(modifier: Modifier, phase: PhaseStats?, top: @Composable () -> Unit) =
    Column(modifier, horizontalAlignment = Alignment.End) {
        top()
        phase?.let {
            Text(
                stringResource(R.string.rallies_detail, it.won.toInt(), it.rallies.toInt()),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = TextAlign.End,
            )
        }
    }

/** The points won minus the lost ones, with the sign when positive. */
internal fun signed(balance: Long) = if (balance > 0) "+$balance" else balance.toString()

@Composable
private fun Tile(modifier: Modifier, content: @Composable () -> Unit) = Surface(
    modifier.semantics(mergeDescendants = true) {},
    shape = MaterialTheme.shapes.medium,
    color = MaterialTheme.colorScheme.surfaceVariant,
) {
    Column(Modifier.padding(12.dp)) { content() }
}

/** The texts of a tile wrap: the large system font makes them longer
 * than the tile. */
@Composable
private fun TileLabel(text: String) = Text(
    text,
    style = MaterialTheme.typography.labelMedium,
    color = MaterialTheme.colorScheme.onSurfaceVariant,
)

@Composable
private fun TileValue(text: String, color: Color?) = Text(
    text,
    style = MaterialTheme.typography.headlineMedium,
    fontWeight = FontWeight.Bold,
    color = color ?: MaterialTheme.colorScheme.onSurface,
)

@Composable
private fun TileDetail(text: String, spoken: String? = null) = Text(
    text,
    when {
        spoken != null -> Modifier.readAs(spoken)
        text.isEmpty() -> Modifier.notRead()
        else -> Modifier
    },
    style = MaterialTheme.typography.labelSmall,
    color = MaterialTheme.colorScheme.onSurfaceVariant,
)

@Composable
private fun PhaseTile(label: String, phase: PhaseStats?, modifier: Modifier) =
    Tile(modifier) {
        TileLabel(label)
        TileValue(pct(phase?.percentage), phase?.grade?.let { gradeColor(it) })
        TileDetail(phase?.let { stringResource(R.string.rallies_detail, it.won.toInt(), it.rallies.toInt()) } ?: "")
    }

@Composable
private fun FundamentalTile(f: FundamentalStats, modifier: Modifier) = Tile(modifier) {
    TileLabel(f.name.replaceFirstChar { it.uppercase() })
    TileValue(pct(f.efficiency), f.grade?.let { gradeColor(it) })
    TileDetail(
        if (f.total > 0u) stringResource(R.string.touches_detail, f.total.toInt(), f.perfect.toInt(), f.errors.toInt())
        else "",
        spoken = stringResource(R.string.touches_spoken, f.total.toInt(), f.perfect.toInt(), f.errors.toInt()).takeIf { f.total > 0u },
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
internal fun Distribution(stats: LiveStats) {
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
        else -> DistributionCourt { z -> ZoneCell(z, zones.find { it.zone.toInt() == z }) }
    }
    summary?.let { AttackLine(stringResource(AttackKind.entries[kind].title), it) }
    if (AttackKind.entries[kind] == AttackKind.All) {
        stats.firstAttack?.let { AttackLine(stringResource(R.string.first_attack), it) }
        stats.counterAttack?.let { AttackLine(stringResource(R.string.counter_attack), it) }
    }
}

@Composable
private fun AttackLine(label: String, attacks: AttackStats) = Row(verticalAlignment = Alignment.CenterVertically) {
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
        color = attacks.grade?.let { gradeColor(it) } ?: MaterialTheme.colorScheme.onSurface,
    )
}

/** Half a court, the net on top: 4 3 2 in the front row, 8 (pipe) and 9
 * (from 1) behind the attack line; what each zone shows in its cell. */
@Composable
fun DistributionCourt(cell: @Composable (zone: Int) -> Unit) {
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
                            zone?.let { cell(it) }
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ZoneCell(zone: Int, stats: ZoneStats?) = Column(horizontalAlignment = Alignment.CenterHorizontally) {
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
            color = it.attacks.grade?.let { g -> gradeColor(g) } ?: MaterialTheme.colorScheme.surfaceVariant,
            contentColor = if (it.attacks.grade != null) Color.Black else MaterialTheme.colorScheme.onSurface,
        ) {
            Text(
                pct(it.attacks.efficiency),
                Modifier.padding(horizontal = 8.dp),
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.Bold,
            )
        }
        Text(
            stringResource(R.string.zone_attacks, it.attacks.total.toInt()),
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
internal fun Errors(stats: LiveStats) {
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
        Text(title, Modifier.weight(1f).title(), style = MaterialTheme.typography.titleMedium)
        Text(total.toString(), style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.Bold, color = color)
    }
    errors.forEach { e ->
        Row(Modifier.semantics(mergeDescendants = true) {}, verticalAlignment = Alignment.CenterVertically) {
            Text(
                names[e.event]?.let { stringResource(it) } ?: e.event.name,
                Modifier.width(132.dp),
                style = MaterialTheme.typography.bodyMedium,
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

// --- trend ------------------------------------------------------------

/** How each figure went in each set, a line per set from its first rally,
 * to compare them, each set shown or hidden by its chip; the match so far
 * dashed across; its direct (unforced) errors, a red line each across (as
 * on the desktop). So far, or of the last 10 touches. Under them, the
 * points of each set in order. A rally a column, the same in the charts and
 * in the points, as wide as the screen allows but never narrower than a
 * score: past the screen they all scroll across together. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun Trend(stats: LiveStats) {
    var recent by remember { mutableStateOf(false) }
    // the sets whose lines are left out
    var hidden by remember { mutableStateOf(setOf<Int>()) }
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(selected = !recent, onClick = { recent = false }, label = { Text(stringResource(R.string.trend_so_far)) })
        FilterChip(selected = recent, onClick = { recent = true }, label = { Text(stringResource(R.string.trend_recent)) })
    }
    if (stats.trend.all { line -> line.sets.all { it.values.isEmpty() } }) {
        Text(stringResource(R.string.no_rallies), color = MaterialTheme.colorScheme.onSurfaceVariant)
        return
    }
    // the sets: a chip each, in the color of its line
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        stats.trend.firstOrNull()?.sets.orEmpty().map { it.setNumber.toInt() }.forEach { n ->
            FilterChip(
                selected = n !in hidden,
                onClick = { hidden = if (n in hidden) hidden - n else hidden + n },
                label = { Text(stringResource(R.string.set_n, n)) },
                leadingIcon = { Box(Modifier.size(width = 12.dp, height = 3.dp).background(setColor(n))) },
            )
        }
    }
    TrendLegend(stats)
    // the rallies of the longest set, at least 25: a column each
    val columns = maxOf(
        25,
        stats.trend.flatMap { line -> line.sets.map { it.values.size } }.maxOrNull() ?: 0,
        stats.points.maxOfOrNull { it.points.size } ?: 0,
    )
    // a score whole in its column, whatever the font of the phone
    val measurer = rememberTextMeasurer()
    val score = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Bold)
    val density = LocalDensity.current
    val narrowest = with(density) { measurer.measure("88", score).size.width.toDp() } + 8.dp
    // one scroll for all of them
    val across = rememberScrollState()
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val column = maxOf(narrowest, maxWidth / columns)
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            stats.trend.forEach { TrendChart(it, recent, hidden, columns, column, across) }
            // the sets hidden left out of the points too
            val points = stats.points.filter { it.points.isNotEmpty() && it.setNumber.toInt() !in hidden }
            if (points.isNotEmpty()) PointSequence(points, column * columns, column, across)
        }
    }
}

/** The points of each set in order, as on the score sheet: two rows, ours
 * and theirs (their colors in the legend), a rally a column of `column`
 * (the columns of the charts over them), the score in the row of the team
 * scoring it; the set over its rows; the sets shown only. TalkBack reads
 * the score of each set as it went. */
@Composable
private fun PointSequence(points: List<SetPoints>, width: Dp, column: Dp, across: ScrollState) =
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(stringResource(R.string.point_sequence), Modifier.title(), style = MaterialTheme.typography.titleSmall)
        points.forEach { set ->
            Text(
                stringResource(R.string.set_n, set.setNumber.toInt()),
                style = MaterialTheme.typography.labelSmall,
                color = setColor(set.setNumber.toInt()),
            )
            // the score after each point, ours first
            val said = set.points.withIndex().joinToString(", ") { (i, p) ->
                val other = set.points.take(i).count { it.us != p.us }
                if (p.us) "${p.score}-$other" else "$other-${p.score}"
            }
            Column(
                Modifier.fillMaxWidth().horizontalScroll(across).testTag("rallies")
                    .readAs(stringResource(R.string.set_n, set.setNumber.toInt()) + ": " + said),
                verticalArrangement = Arrangement.spacedBy(2.dp),
            ) {
                listOf(true to TeamUs, false to TeamThem).forEach { (us, color) ->
                    Row(Modifier.width(width)) {
                        set.points.forEach { p ->
                            Box(Modifier.width(column).padding(horizontal = 1.dp)) {
                                // the empty ones as high, their score unseen
                                Box(
                                    Modifier.fillMaxWidth().background(
                                        if (p.us == us) color else Color.Transparent,
                                        RoundedCornerShape(4.dp),
                                    ).padding(vertical = 2.dp),
                                    contentAlignment = Alignment.Center,
                                ) {
                                    Text(
                                        p.score.toString(),
                                        style = MaterialTheme.typography.labelSmall,
                                        fontWeight = FontWeight.Bold,
                                        color = if (p.us == us) MaterialTheme.colorScheme.surface else Color.Transparent,
                                        maxLines = 1,
                                    )
                                }
                            }
                        }
                    }
                }
            }
        }
    }

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun TrendLegend(stats: LiveStats) = FlowRow(
    // each chart says its sets
    Modifier.notRead(),
    horizontalArrangement = Arrangement.spacedBy(12.dp),
    itemVerticalAlignment = Alignment.CenterVertically,
) {
    Text(
        "- - " + stringResource(R.string.whole_match),
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Text("│ " + stringResource(R.string.trend_direct), style = MaterialTheme.typography.labelSmall, color = Lost)
    // the colors of the two teams in the points
    listOf(stats.teamName to TeamUs, stats.opponent to TeamThem).forEach { (name, color) ->
        Text("■ $name", style = MaterialTheme.typography.labelSmall, color = color)
    }
}

@Composable
private fun TrendChart(
    line: TrendLine,
    recent: Boolean,
    hidden: Set<Int>,
    columns: Int,
    column: Dp,
    across: ScrollState,
) = Column(Modifier.semantics(mergeDescendants = true) {}) {
    val overall = if (recent) line.overallRecent else line.overall
    val overallGrade = if (recent) line.overallRecentGrade else line.overallGrade
    Row(verticalAlignment = Alignment.Bottom) {
        Text(line.name.replaceFirstChar { it.uppercase() }, Modifier.weight(1f), style = MaterialTheme.typography.titleSmall)
        Text(
            pct(overall),
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
            color = overallGrade?.let { gradeColor(it) } ?: MaterialTheme.colorScheme.onSurface,
        )
    }
    // each set: its number, its values rally by rally (from the first)
    val all = line.sets.map { set -> set.setNumber.toInt() to if (recent) set.recent else set.values }
    // the hidden ones left out, the axis as with all of them
    val sets = all.filter { it.first !in hidden }
    val axis = MaterialTheme.colorScheme.outline
    val known = all.flatMap { it.second }.filterNotNull() + listOfNotNull(overall)
    // always 0 in sight
    val low = minOf(0.0, known.minOrNull() ?: 0.0)
    val high = maxOf(low + 10.0, known.maxOrNull() ?: 0.0)
    val points = columns.toDouble()
    // the direct errors in each rally of the sets, of every set shown
    val bins = IntArray(columns.coerceAtLeast(1))
    line.sets.filter { it.setNumber.toInt() !in hidden }.forEach { set ->
        set.unforced.forEachIndexed { i, n ->
            bins[i.coerceIn(0, bins.size - 1)] += n.toInt()
        }
    }
    // the chart and its rallies under it, scrolling with the others
    Column(Modifier.fillMaxWidth().horizontalScroll(across).testTag("rallies")) {
    Canvas(Modifier.width(column * columns).height(88.dp).padding(vertical = 4.dp)) {
        // a rally (from 1) in the middle of its column
        fun x(p: Double) = (size.width * (p - 0.5) / points).toFloat()
        fun y(v: Double) = (size.height * (1 - (v - low) / (high - low))).toFloat()
        val dashed = PathEffect.dashPathEffect(floatArrayOf(8f, 8f))
        drawLine(axis, Offset(0f, y(0.0)), Offset(size.width, y(0.0)), strokeWidth = 1.dp.toPx())
        // the first rally and every 5
        (listOf(1) + (5..columns step 5)).forEach { p ->
            drawLine(axis.copy(alpha = 0.3f), Offset(x(p.toDouble()), 0f), Offset(x(p.toDouble()), size.height), 1.dp.toPx())
        }
        // the direct errors, a red line across in the column of their rally
        bins.forEachIndexed { i, n ->
            if (n > 0) {
                drawLine(Lost.copy(alpha = 0.8f), Offset(x(i + 1.0), 0f), Offset(x(i + 1.0), size.height), 1.5f.dp.toPx())
            }
        }
        overall?.let {
            drawLine(Color.White.copy(alpha = 0.6f), Offset(0f, y(it)), Offset(size.width, y(it)), 1.5f.dp.toPx(), pathEffect = dashed)
        }
        sets.forEach { (setNumber, values) ->
            val path = Path()
            var drawing = false
            values.forEachIndexed { i, v ->
                val px = x(i + 1.0)
                when {
                    v == null -> drawing = false
                    drawing -> path.lineTo(px, y(v))
                    else -> {
                        path.moveTo(px, y(v))
                        drawing = true
                    }
                }
            }
            val color = setColor(setNumber)
            drawPath(path, color, style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round))
            values.withIndex().lastOrNull { it.value != null }?.let { (i, v) ->
                drawCircle(color, 3.dp.toPx(), Offset(x(i + 1.0), y(v ?: 0.0)))
            }
        }
    }
    // the rallies: the first and every 5, under their columns
    Box(Modifier.width(column * columns).notRead()) {
        (listOf(1) + (5..columns step 5)).forEach { p ->
            Box(Modifier.offset(x = column * (p - 1)).width(column), contentAlignment = Alignment.Center) {
                Text(
                    p.toString(),
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                )
            }
        }
    }
    }
    // for TalkBack, where each set got to, and the errors
    Box(
        Modifier.readAs(
            (sets.mapNotNull { (n, values) -> values.lastOrNull { it != null }?.let { stringResource(R.string.set_n, n) + " " + pct(it) } } +
                listOf(stringResource(R.string.trend_errors_said, bins.sum())))
                .joinToString(", "),
        ),
    )
}
