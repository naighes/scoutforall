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
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.AnalysedMatch
import io.github.naighes.scout4all.core.AnalysisPeriod
import io.github.naighes.scout4all.core.MatchTrend
import io.github.naighes.scout4all.core.MobileException
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
private enum class PeriodKind { LastMonth, LastMatches, Between }

/** The statistics of a team over the matches of a period: as for a match,
 * each figure match by match, a fundamental in detail; narrowed by the
 * filters, the situation of the game too, on every page. */
@OptIn(ExperimentalMaterial3Api::class, ExperimentalLayoutApi::class)
@Composable
fun AnalysisScreen(teamId: String, onBack: () -> Unit, onWatch: (VideoRequest.Clips) -> Unit = {}) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    var kind by rememberSaveable { mutableStateOf(PeriodKind.LastMonth) }
    var count by rememberSaveable { mutableIntStateOf(LAST_MATCHES) }
    // the digits typed, YYYYMMDD
    var from by rememberSaveable { mutableStateOf("") }
    var to by rememberSaveable { mutableStateOf("") }
    var page by rememberSaveable { mutableIntStateOf(0) }
    var query by remember { mutableStateOf(DetailQuery()) }
    // the period read: two days only once both are
    var period by remember { mutableStateOf<AnalysisPeriod>(AnalysisPeriod.LastMonth) }
    LaunchedEffect(kind, count, from, to) {
        when (kind) {
            PeriodKind.LastMonth -> period = AnalysisPeriod.LastMonth
            PeriodKind.LastMatches -> period = AnalysisPeriod.LastMatches(count.toUInt())
            PeriodKind.Between -> {
                val days = listOf(from, to).map(::dayOf)
                if (days.all { it != null }) period = AnalysisPeriod.Between(days[0].toString(), days[1].toString())
            }
        }
    }
    // the last one read stays while the next is
    var analysis by remember { mutableStateOf<TeamAnalysis?>(null) }
    var failed by remember { mutableStateOf<String?>(null) }
    LaunchedEffect(period, query) {
        try {
            analysis = app.core { it.teamAnalysis(teamId, period, query.event, query.filter) }
            failed = null
        } catch (e: MobileException) {
            failed = e.reason
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.analysis), Modifier.title()) },
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
                        Tab(selected = page == i, onClick = { page = i }, text = { Text(stringResource(p.title)) })
                    }
                }
                Column(
                    Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(16.dp),
                    verticalArrangement = Arrangement.spacedBy(16.dp),
                ) {
                    PeriodChooser(
                        kind,
                        count,
                        from,
                        to,
                        onKind = { k ->
                            // the days from the period shown
                            if (k == PeriodKind.Between && from.isEmpty() && to.isEmpty()) {
                                from = a.from?.filter(Char::isDigit).orEmpty()
                                to = a.to?.filter(Char::isDigit).orEmpty()
                            }
                            kind = k
                        },
                        onCount = { count = it.coerceIn(1, 99) },
                        onDays = { f, t ->
                            from = f
                            to = t
                        },
                    )
                    PeriodLine(a)
                    failed?.let { Text(it, color = MaterialTheme.colorScheme.error) }
                    if (a.matches.isEmpty()) {
                        Text(stringResource(R.string.no_matches_period), color = MaterialTheme.colorScheme.onSurfaceVariant)
                    } else {
                        DetailFilters(a.detail, a.situation, query.filter) { query = query.copy(filter = it) }
                        when (pages[page]) {
                            AnalysisPage.Summary -> {
                                Summary(a.summary)
                                Distribution(a.summary)
                                Errors(a.summary)
                            }
                            AnalysisPage.Trend -> {
                                TrendLegend(a.matches.size)
                                a.trend.forEach { MatchTrendChart(it, a.matches) }
                            }
                            AnalysisPage.Detail -> Detail(a.detail, query, a.situation, filters = false, watch = teamWatch(teamId, period, onWatch)) { query = it }
                        }
                    }
                }
            }
        }
    }
}

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

@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun PeriodChooser(
    kind: PeriodKind,
    count: Int,
    from: String,
    to: String,
    onKind: (PeriodKind) -> Unit,
    onCount: (Int) -> Unit,
    onDays: (String, String) -> Unit,
) = Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
    FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.Center) {
        FilterChip(
            selected = kind == PeriodKind.LastMonth,
            onClick = { onKind(PeriodKind.LastMonth) },
            label = { Text(stringResource(R.string.last_month)) },
        )
        FilterChip(
            selected = kind == PeriodKind.LastMatches,
            onClick = { onKind(PeriodKind.LastMatches) },
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
            onClick = { onKind(PeriodKind.Between) },
            label = { Text(stringResource(R.string.between)) },
        )
    }
    if (kind == PeriodKind.Between) DaysChooser(from, to, onDays)
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
        supportingText = if (wrong) ({ Text(stringResource(R.string.not_a_day)) }) else null,
        visualTransformation = DayMask,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
    )
}

/** The days of the period, how many matches, the ones left out. */
@Composable
private fun PeriodLine(a: TeamAnalysis) = Column {
    val days = listOfNotNull(a.from, a.to).joinToString(" → ")
    Text(
        listOf(days, stringResource(R.string.matches_count, a.matches.size)).filter { it.isNotEmpty() }.joinToString(" · "),
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    if (a.skipped.isNotEmpty()) {
        Text(
            stringResource(R.string.skipped, a.skipped.joinToString(", ")),
            style = MaterialTheme.typography.labelMedium,
            color = Lost,
        )
    }
}

/** The lines of the charts, said once. */
@Composable
private fun TrendLegend(matches: Int) = FlowRow(
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
private fun MatchTrendChart(trend: MatchTrend, matches: List<AnalysedMatch>) = Column(Modifier.semantics(mergeDescendants = true) {}) {
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
