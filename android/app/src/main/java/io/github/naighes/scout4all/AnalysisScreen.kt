package io.github.naighes.scout4all

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.DateRange
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.DateRangePicker
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberDateRangePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.Path
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.StrokeCap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.OffsetMapping
import androidx.compose.ui.text.input.TransformedText
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.DetailFilter
import io.github.naighes.scout4all.core.AnalysedMatch
import io.github.naighes.scout4all.core.AnalysisPeriod
import io.github.naighes.scout4all.core.CompetitionChoice
import io.github.naighes.scout4all.core.MatchTrend
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.LiveStats
import io.github.naighes.scout4all.core.ScoutedOpponent
import io.github.naighes.scout4all.core.ScoutingPlayer
import io.github.naighes.scout4all.core.TeamAnalysis
import java.time.DateTimeException
import java.time.Instant
import java.time.LocalDate
import java.time.ZoneOffset
import java.time.format.DateTimeFormatter
import kotlin.math.abs

/** The matches of the moving average, as in the core. */
private const val MOVING_WINDOW = 3

/** The last matches at first. */
private const val LAST_MATCHES = 5

private enum class AnalysisPage(val title: Int) {
    Summary(R.string.stats_summary),
    Trend(R.string.stats_trend),
    Detail(R.string.stats_detail),
}

/** How the period is chosen. */
internal enum class PeriodKind { LastMonth, Season, LastMatches, Between }

/** The period as chosen: the days are the digits typed, YYYYMMDD. */
internal data class PeriodChoice(
    val kind: PeriodKind,
    val count: Int = LAST_MATCHES,
    val from: String = "",
    val to: String = "",
) {
    /** Null with two days not both written: the one before stays. */
    fun period(): AnalysisPeriod? = when (kind) {
        PeriodKind.LastMonth -> AnalysisPeriod.LastMonth
        PeriodKind.Season -> AnalysisPeriod.Season
        PeriodKind.LastMatches -> AnalysisPeriod.LastMatches(count.toUInt())
        PeriodKind.Between -> listOf(from, to).map(::dayOf).let { (first, last) ->
            if (first != null && last != null) AnalysisPeriod.Between(first.toString(), last.toString()) else null
        }
    }

    companion object {
        val Saver = listSaver<PeriodChoice, Any>(
            save = { listOf(it.kind.name, it.count, it.from, it.to) },
            restore = { PeriodChoice(PeriodKind.valueOf(it[0] as String), it[1] as Int, it[2] as String, it[3] as String) },
        )
    }
}

/** The analysis narrowed to a player, from their profile, with its filters. */
internal data class LockedAnalysis(val player: String, val choice: PeriodChoice, val competition: String?)

/** The statistics of a team over the matches of a period: as for a match,
 * each figure match by match, a fundamental in detail; narrowed by the
 * filters, the situation of the game too, on every page. */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
internal fun AnalysisScreen(
    teamId: String,
    onBack: () -> Unit,
    onWatch: (VideoRequest.Clips) -> Unit = {},
    locked: LockedAnalysis? = null,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    var choice by rememberSaveable(stateSaver = PeriodChoice.Saver) {
        mutableStateOf(locked?.choice ?: PeriodChoice(PeriodKind.LastMonth))
    }
    var page by rememberSaveable { mutableIntStateOf(0) }
    // the competition the matches are narrowed to, none for every one
    var competition by rememberSaveable { mutableStateOf(locked?.competition) }
    val players = listOfNotNull(locked?.player)
    var query by remember { mutableStateOf(DetailQuery(filter = DetailFilter(null, players = players))) }
    // the player stays the one of the profile, whatever the page picks
    val ask = { q: DetailQuery ->
        query = if (locked == null) q else q.copy(filter = q.filter.copy(players = players))
    }
    // the player by name, in the title
    var name by remember { mutableStateOf<String?>(null) }
    // the opponent the analysis is seen from, none for the team itself
    var opponent by rememberSaveable { mutableStateOf<String?>(null) }
    // the opponent and the team side by side, in their matches
    var compare by rememberSaveable { mutableStateOf(false) }
    var ours by remember { mutableStateOf("") }
    var opponents by remember { mutableStateOf<List<ScoutedOpponent>>(emptyList()) }
    LaunchedEffect(teamId) {
        try {
            val team = app.core { it.team(teamId) }
            ours = team.name
            name = team.players.firstOrNull { it.id == locked?.player }?.name
            // a player is of the team only
            if (locked == null) opponents = app.core { it.analysisOpponents(teamId) }
        } catch (_: MobileException) {
        }
    }
    // the period read: two days only once both are
    var period by remember { mutableStateOf(choice.period() ?: AnalysisPeriod.LastMonth) }
    LaunchedEffect(choice) { choice.period()?.let { period = it } }
    // the last one read stays while the next is
    var analysis by remember { mutableStateOf<TeamAnalysis?>(null) }
    var failed by remember { mutableStateOf<String?>(null) }
    // the team over the matches of the opponent, to put beside its own
    var versus by remember { mutableStateOf<LiveStats?>(null) }
    LaunchedEffect(period, competition, query, opponent, compare) {
        try {
            versus = opponent?.takeIf { compare }?.let { o ->
                app.core { it.analysisAgainst(teamId, o, period, competition, query.event, query.filter).summary }
            }
            analysis = app.core {
                opponent?.let { o -> it.opponentAnalysis(teamId, o, period, competition, query.event, query.filter) }
                    ?: it.teamAnalysis(teamId, period, competition, query.event, query.filter)
            }
            failed = null
        } catch (e: MobileException) {
            failed = e.reason
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    val title = listOfNotNull(name, stringResource(R.string.analysis)).joinToString(" – ")
                    Text(title, Modifier.title(), maxLines = 1, overflow = TextOverflow.Ellipsis)
                },
                navigationIcon = { BackButton(onBack) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        val a = analysis
        when {
            a == null && failed != null -> Centered(modifier) { Text(failed.orEmpty()) }
            a == null -> Centered(modifier) { CircularProgressIndicator() }
            else -> Column(modifier) {
                val pages = AnalysisPage.entries
                PrimaryTabRow(selectedTabIndex = page) {
                    pages.forEachIndexed { i, p ->
                        // only the summary compares the matches of two teams
                        Tab(selected = page == i, onClick = { page = i; if (p != AnalysisPage.Summary) compare = false }, text = { Text(stringResource(p.title)) })
                    }
                }
                Column(
                    Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    if (opponents.isNotEmpty()) {
                        SeenFromChooser(
                            ours,
                            opponents,
                            opponent,
                            compare,
                            comparable = pages[page] == AnalysisPage.Summary,
                            onCompare = {
                                // the players chosen are of one of the two teams
                                query = query.withoutPlayers()
                                compare = true
                            },
                        ) {
                            // the players chosen are of the other team
                            query = query.withoutPlayers()
                            opponent = it
                            compare = false
                        }
                    }
                    // offered with more than one, or while one is chosen
                    if (a.competitions.size > 1 || competition != null) {
                        CompetitionChooser(a.competitions, competition) { competition = it }
                    }
                    PeriodChooser(choice, a.from, a.to) { choice = it }
                    PeriodLine(a.from, a.to, a.matches.size, a.skipped)
                    failed?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                    if (a.matches.isEmpty()) {
                        Text(stringResource(R.string.no_matches_period), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    } else {
                        DetailFilters(
                            a.detail,
                            a.situation,
                            query.filter,
                            query.event.takeIf { pages[page] == AnalysisPage.Detail },
                            filterPlayers(a, pages[page] == AnalysisPage.Detail, compare).takeIf { locked == null },
                        ) { ask(query.copy(filter = it)) }
                        when (pages[page]) {
                            AnalysisPage.Summary -> if (compare) {
                                versus?.let { Comparison(it, a.summary) }
                                    ?: Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
                            } else {
                                Summary(a.summary)
                                Distribution(a.summary)
                                Errors(a.summary)
                            }
                            AnalysisPage.Trend -> {
                                TrendLegend(a.matches.size)
                                a.trend.forEach { MatchTrendChart(it, a.matches) }
                            }
                            AnalysisPage.Detail -> Detail(a.detail, query, a.situation, filters = false, watch = teamWatch(teamId, period, competition, opponent, onWatch)) { ask(it) }
                        }
                    }
                }
            }
        }
    }
}

/** The players the filter offers: on the detail the ones of its
 * fundamental, on the other pages every one with a touch of any (the libero
 * too, who never serves); none comparing two teams, whose players aren't
 * the same. */
internal fun filterPlayers(analysis: TeamAnalysis, detail: Boolean, compare: Boolean): List<ScoutingPlayer>? = when {
    compare -> null
    detail -> analysis.detail.players
    else -> analysis.players
}

/** The query with no player chosen. */
private fun DetailQuery.withoutPlayers() = copy(filter = filter.copy(players = emptyList()))

/** The day of the digits typed, YYYYMMDD; none until it's whole and a day. */
internal fun dayOf(digits: String): LocalDate? = when (digits.length) {
    8 -> try {
        LocalDate.of(digits.take(4).toInt(), digits.substring(4, 6).toInt(), digits.substring(6).toInt())
    } catch (_: DateTimeException) {
        null
    }
    else -> null
}

/** The digits of a day typed, shown as YYYY-MM-DD. */
internal object DayMask : VisualTransformation {
    override fun filter(text: AnnotatedString): TransformedText {
        val digits = text.text
        val shown = digits.mapIndexed { i, c -> if (i == 4 || i == 6) "-$c" else "$c" }.joinToString("")
        return TransformedText(
            AnnotatedString(shown),
            object : OffsetMapping {
                override fun originalToTransformed(offset: Int) = offset + listOf(4, 6).count { offset > it }
                override fun transformedToOriginal(offset: Int) =
                    (offset - listOf(5, 8).count { offset >= it }).coerceIn(0, digits.length)
            },
        )
    }
}

/** The team itself or an opponent scouted on both sides, to see the analysis from. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SeenFromChooser(
    ours: String,
    opponents: List<ScoutedOpponent>,
    chosen: String?,
    compare: Boolean,
    comparable: Boolean,
    onCompare: () -> Unit,
    onChoose: (String?) -> Unit,
) = Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
    Text(stringResource(R.string.seen_from), style = MaterialTheme.typography.labelLarge)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(selected = chosen == null, onClick = { onChoose(null) }, label = { Text(ours) })
        opponents.forEach {
            FilterChip(selected = chosen == it.id && !compare, onClick = { onChoose(it.id) }, label = { Text(it.name) })
        }
        // the opponent chosen and the team side by side, on the summary
        if (chosen != null && comparable) {
            FilterChip(selected = compare, onClick = onCompare, label = { Text(stringResource(R.string.compare)) })
        }
    }
}

/** The competitions of the matches scouted, one to narrow them to. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun CompetitionChooser(
    competitions: List<CompetitionChoice>,
    chosen: String?,
    onChoose: (String?) -> Unit,
) = Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
    Text(stringResource(R.string.competition), style = MaterialTheme.typography.labelLarge)
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        FilterChip(
            selected = chosen == null,
            onClick = { onChoose(null) },
            label = { Text(stringResource(R.string.all_competitions)) },
        )
        competitions.forEach {
            FilterChip(selected = chosen == it.id, onClick = { onChoose(it.id) }, label = { Text(it.name) })
        }
    }
}

/** The kinds of period; [shownFrom] and [shownTo], the days of the one
 * shown, start the two days. */
@OptIn(ExperimentalLayoutApi::class)
@Composable
internal fun PeriodChooser(
    choice: PeriodChoice,
    shownFrom: String?,
    shownTo: String?,
    onChange: (PeriodChoice) -> Unit,
) = Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
    val kind = choice.kind
    val count = choice.count
    val onCount = { n: Int -> onChange(choice.copy(count = n.coerceIn(1, 99))) }
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.Center) {
        FilterChip(
            selected = kind == PeriodKind.LastMonth,
            onClick = { onChange(choice.copy(kind = PeriodKind.LastMonth)) },
            label = { Text(stringResource(R.string.last_month)) },
        )
        FilterChip(
            selected = kind == PeriodKind.Season,
            onClick = { onChange(choice.copy(kind = PeriodKind.Season)) },
            label = { Text(stringResource(R.string.season_period)) },
        )
        FilterChip(
            selected = kind == PeriodKind.LastMatches,
            onClick = { onChange(choice.copy(kind = PeriodKind.LastMatches)) },
            label = { Text(stringResource(R.string.last_matches, count)) },
        )
        if (kind == PeriodKind.LastMatches) {
            TextButton(onClick = { onCount(count - 1) }, enabled = count > 1) {
                Text("−", Modifier.readAs(stringResource(R.string.fewer_matches)), style = MaterialTheme.typography.titleLarge)
            }
            IconButton(onClick = { onCount(count + 1) }) {
                Icon(Icons.Filled.Add, contentDescription = stringResource(R.string.more_matches))
            }
        }
        FilterChip(
            selected = kind == PeriodKind.Between,
            onClick = {
                val empty = choice.from.isEmpty() && choice.to.isEmpty()
                onChange(
                    choice.copy(
                        kind = PeriodKind.Between,
                        from = if (empty) shownFrom?.filter(Char::isDigit).orEmpty() else choice.from,
                        to = if (empty) shownTo?.filter(Char::isDigit).orEmpty() else choice.to,
                    ),
                )
            },
            label = { Text(stringResource(R.string.between)) },
        )
    }
    if (kind == PeriodKind.Between) DaysChooser(choice.from, choice.to) { f, t -> onChange(choice.copy(from = f, to = t)) }
}

/** The two days, typed or chosen on the calendar. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun DaysChooser(from: String, to: String, onDays: (String, String) -> Unit) {
    var picking by rememberSaveable { mutableStateOf(false) }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
        TypedDayField(stringResource(R.string.from_day), from, Modifier.weight(1f)) { onDays(it, to) }
        TypedDayField(stringResource(R.string.to_day), to, Modifier.weight(1f)) { onDays(from, it) }
        IconButton(onClick = { picking = true }) {
            Icon(Icons.Filled.DateRange, contentDescription = stringResource(R.string.pick_days))
        }
    }
    if (picking) {
        // the picker counts days in UTC
        val millis = { day: LocalDate? -> day?.atStartOfDay(ZoneOffset.UTC)?.toInstant()?.toEpochMilli() }
        val picker = rememberDateRangePickerState(
            initialSelectedStartDateMillis = millis(dayOf(from)),
            initialSelectedEndDateMillis = millis(dayOf(to)),
        )
        val digits = { m: Long -> Instant.ofEpochMilli(m).atZone(ZoneOffset.UTC).toLocalDate().format(DateTimeFormatter.BASIC_ISO_DATE) }
        DatePickerDialog(
            onDismissRequest = { picking = false },
            confirmButton = {
                TextButton(
                    onClick = {
                        picker.selectedStartDateMillis?.let { start ->
                            // one day alone: that day
                            val end = picker.selectedEndDateMillis ?: start
                            onDays(digits(start), digits(end))
                        }
                        picking = false
                    },
                ) { Text(stringResource(R.string.ok)) }
            },
            dismissButton = {
                TextButton(onClick = { picking = false }) { Text(stringResource(R.string.cancel)) }
            },
        ) { DateRangePicker(picker, Modifier.weight(1f)) }
    }
}

/** A day typed: digits only, the dashes put in (the period of the
 * analysis, beside its calendar of a range: not the `DayField` of the
 * forms). */
@Composable
private fun TypedDayField(label: String, digits: String, modifier: Modifier, onDigits: (String) -> Unit) {
    val wrong = digits.length == 8 && dayOf(digits) == null
    OutlinedTextField(
        value = digits,
        onValueChange = { onDigits(it.filter(Char::isDigit).take(8)) },
        modifier = modifier,
        label = { Text(label) },
        placeholder = { Text(stringResource(R.string.date_placeholder)) },
        singleLine = true,
        isError = wrong,
        supportingText = noteOf(if (wrong) stringResource(R.string.not_a_day) else null),
        visualTransformation = DayMask,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
    )
}

/** The days of the period, how many matches, the ones left out. */
@Composable
internal fun PeriodLine(from: String?, to: String?, matches: Int, skipped: List<String>) = Column {
    val days = listOfNotNull(from, to).joinToString(" → ")
    Text(
        listOf(days, stringResource(R.string.matches_count, matches)).filter { it.isNotEmpty() }.joinToString(" · "),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    if (skipped.isNotEmpty()) {
        Text(
            stringResource(R.string.skipped, skipped.joinToString(", ")),
            style = MaterialTheme.typography.labelMedium,
            color = Lost,
        )
    }
}

/** The lines of the charts, said once. */
@Composable
internal fun TrendLegend(matches: Int) = FlowRow(
    Modifier.notRead(),
    horizontalArrangement = Arrangement.spacedBy(12.dp),
) {
    val legend = @Composable { text: String, mark: @Composable () -> Unit ->
        Row(verticalAlignment = Alignment.CenterVertically) {
            mark()
            Text(" $text", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
    legend(stringResource(R.string.per_match)) { Box(Modifier.size(6.dp).background(MaterialTheme.colorScheme.onSurface, CircleShape)) }
    legend(stringResource(R.string.moving_average, MOVING_WINDOW)) { Box(Modifier.size(width = 12.dp, height = 3.dp).background(Fair)) }
    legend("- - " + stringResource(R.string.matches_count, matches)) {}
}

private val shortDay = DateTimeFormatter.ofPattern("dd/MM")

internal fun shortDay(day: String): String = try {
    LocalDate.parse(day).format(shortDay)
} catch (_: DateTimeException) {
    day
}

/** A figure match by match: each value a dot, the moving average a thick
 * line, the whole period dashed. A touch on
 * a match says it. */
@Composable
internal fun MatchTrendChart(trend: MatchTrend, matches: List<AnalysedMatch>) = Column(Modifier.semantics(mergeDescendants = true) {}) {
    var chosen by remember(matches) { mutableStateOf<Int?>(null) }
    Row(verticalAlignment = Alignment.Bottom) {
        Text(trend.name.replaceFirstChar { it.uppercase() }, Modifier.weight(1f), style = MaterialTheme.typography.titleSmall)
        Text(
            pct(trend.overall),
            style = MaterialTheme.typography.titleMedium,
            fontWeight = FontWeight.Bold,
            color = trend.overallGrade?.let { gradeColor(it) } ?: MaterialTheme.colorScheme.onSurface,
        )
    }
    val known = (trend.values + trend.moving).filterNotNull() + listOfNotNull(trend.overall)
    // always 0 in sight
    val low = minOf(0.0, known.minOrNull() ?: 0.0)
    val high = maxOf(low + 10.0, known.maxOrNull() ?: 0.0)
    val axis = MaterialTheme.colorScheme.outline
    val dots = MaterialTheme.colorScheme.onSurface
    val n = matches.size
    Canvas(
        Modifier.fillMaxWidth().height(120.dp).padding(vertical = 6.dp).pointerInput(n) {
            detectTapGestures { tap ->
                val margin = 8.dp.toPx()
                val step = if (n > 1) (size.width - 2 * margin) / (n - 1) else 0f
                chosen = (0 until n).minByOrNull { i -> abs(if (n > 1) margin + i * step - tap.x else size.width / 2f - tap.x) }
            }
        },
    ) {
        val margin = 8.dp.toPx()
        fun x(i: Int) = if (n > 1) margin + i * (size.width - 2 * margin) / (n - 1) else size.width / 2
        fun y(v: Double) = (size.height * (1 - (v - low) / (high - low))).toFloat()
        drawLine(axis, Offset(0f, y(0.0)), Offset(size.width, y(0.0)), strokeWidth = 1.dp.toPx())
        trend.overall?.let {
            val dashed = PathEffect.dashPathEffect(floatArrayOf(8f, 8f))
            drawLine(Color.White.copy(alpha = 0.6f), Offset(0f, y(it)), Offset(size.width, y(it)), 1.5f.dp.toPx(), pathEffect = dashed)
        }
        chosen?.let { drawLine(axis, Offset(x(it), 0f), Offset(x(it), size.height), 1.dp.toPx()) }
        fun line(values: List<Double?>): Path {
            val path = Path()
            var drawing = false
            values.forEachIndexed { i, v ->
                when {
                    v == null -> drawing = false
                    drawing -> path.lineTo(x(i), y(v))
                    else -> {
                        path.moveTo(x(i), y(v))
                        drawing = true
                    }
                }
            }
            return path
        }
        drawPath(line(trend.moving), Fair, style = Stroke(width = 3.dp.toPx(), cap = StrokeCap.Round))
        drawPath(line(trend.values), dots.copy(alpha = 0.7f), style = Stroke(width = 1.dp.toPx()))
        trend.values.forEachIndexed { i, v ->
            v?.let { drawCircle(dots, (if (i == chosen) 5 else 3).dp.toPx(), Offset(x(i), y(it))) }
        }
    }
    // the match touched, or the first and the last day
    val said = matches.mapIndexed { i, m ->
        val value = trend.values.getOrNull(i)
        listOf(
            shortDay(m.day),
            m.opponent,
            pct(value),
            stringResource(R.string.touches, trend.touches.getOrNull(i)?.toInt() ?: 0),
        ).joinToString(" · ")
    }
    when (val i = chosen) {
        null -> Row(Modifier.fillMaxWidth().notRead(), horizontalArrangement = Arrangement.SpaceBetween) {
            listOfNotNull(matches.firstOrNull(), matches.lastOrNull().takeIf { n > 1 }).forEach {
                Text(shortDay(it.day), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        else -> Text(said.getOrElse(i) { "" }, Modifier.notRead(), style = MaterialTheme.typography.labelMedium)
    }
    // for TalkBack, each match
    Box(Modifier.readAs(said.joinToString(", ")))
}
