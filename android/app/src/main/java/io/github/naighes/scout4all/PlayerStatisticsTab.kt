package io.github.naighes.scout4all

import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.EventTypeEnum
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.PlayerFundamental
import io.github.naighes.scout4all.core.PlayerMeasure
import io.github.naighes.scout4all.core.PlayerStatistics

/** The filters of the statistics of a player and what they give, kept while
 * the profile scrolls and switches tabs. */
internal class PlayerStatisticsState(
    private val chosen: MutableState<PeriodChoice>,
    private val narrowed: MutableState<String?>,
) {
    var choice by chosen
    var competition by narrowed
    /** the last one read stays while the next is */
    var read by mutableStateOf<PlayerStatistics?>(null)
    var failed by mutableStateOf<String?>(null)
}

/** The whole season at first; read only while [shown]. */
@Composable
internal fun playerStatisticsState(teamId: String, playerId: String, shown: Boolean): PlayerStatisticsState {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val choice = rememberSaveable(stateSaver = PeriodChoice.Saver) { mutableStateOf(PeriodChoice(PeriodKind.Season)) }
    val competition = rememberSaveable { mutableStateOf<String?>(null) }
    val state = remember(teamId, playerId) { PlayerStatisticsState(choice, competition) }
    var period by remember { mutableStateOf(choice.value.period()) }
    LaunchedEffect(choice.value) { choice.value.period()?.let { period = it } }
    LaunchedEffect(teamId, playerId, shown, period, competition.value) {
        val asked = period ?: return@LaunchedEffect
        if (!shown) return@LaunchedEffect
        try {
            state.read = app.core { it.playerStatistics(teamId, playerId, asked, competition.value) }
            state.failed = null
        } catch (e: MobileException) {
            state.failed = e.reason
        }
    }
    return state
}

/** Two columns of boxes from this width, three from the next. */
private val TWO_COLUMNS = 600.dp
private val THREE_COLUMNS = 900.dp

/** The filters, a box a fundamental, then each fundamental match by match:
 * the figures of the analysis narrowed to the player, which [onDetail]
 * opens. */
@Composable
internal fun PlayerStatisticsTab(state: PlayerStatisticsState, onDetail: () -> Unit) =
    Column(Modifier.fillMaxWidth().padding(16.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        val r = state.read
        if (r != null && (r.competitions.size > 1 || state.competition != null)) {
            CompetitionChooser(r.competitions, state.competition) { state.competition = it }
        }
        PeriodChooser(state.choice, r?.from, r?.to) { state.choice = it }
        state.failed?.let { Text(it, color = MaterialTheme.colorScheme.error) }
        val muted = MaterialTheme.colorScheme.onSurfaceVariant
        when {
            r == null -> if (state.failed == null) {
                Box(Modifier.fillMaxWidth(), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
            }
            else -> {
                PeriodLine(r.from, r.to, r.matches.size, r.skipped)
                when {
                    r.matches.isEmpty() -> Text(stringResource(R.string.no_matches_period), color = muted)
                    r.fundamentals.isEmpty() -> Text(stringResource(R.string.no_events), color = muted)
                    else -> Figures(r, onDetail)
                }
            }
        }
    }

@Composable
private fun Figures(r: PlayerStatistics, onDetail: () -> Unit) {
    OutlinedButton(onClick = onDetail) { Text(stringResource(R.string.stats_detail)) }
    BoxWithConstraints {
        val columns = when {
            maxWidth >= THREE_COLUMNS -> 3
            maxWidth >= TWO_COLUMNS -> 2
            else -> 1
        }
        Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
            r.fundamentals.chunked(columns).forEach { row ->
                Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                    row.forEach { Figure(it, Modifier.weight(1f)) }
                    repeat(columns - row.size) { Spacer(Modifier.weight(1f)) }
                }
            }
        }
    }
    if (r.trend.isNotEmpty()) TrendLegend(r.matches.size)
    // the trends are of the efficiency, whatever the box shows
    val efficiency = stringResource(R.string.efficiency)
    r.trend.forEach { MatchTrendChart(it.copy(name = "${it.name} · $efficiency"), r.matches) }
}

/** A fundamental: its figure large, the touches, the points (the aces of
 * the serve) and the errors. */
@Composable
private fun Figure(f: PlayerFundamental, modifier: Modifier) = OutlinedCard(modifier) {
    Column(Modifier.fillMaxWidth().padding(14.dp), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        val muted = MaterialTheme.colorScheme.onSurfaceVariant
        Text(f.name.replaceFirstChar { it.uppercase() }, style = MaterialTheme.typography.titleMedium)
        Text(
            pct(f.value),
            style = MaterialTheme.typography.displaySmall,
            fontWeight = FontWeight.Bold,
            color = f.grade?.let { gradeColor(it) } ?: MaterialTheme.colorScheme.onSurface,
        )
        Text(
            stringResource(
                when (f.measure) {
                    PlayerMeasure.EFFICIENCY -> R.string.efficiency
                    PlayerMeasure.POSITIVITY -> R.string.positiveness
                },
            ),
            style = MaterialTheme.typography.labelMedium,
            color = muted,
        )
        Text(stringResource(R.string.touches, f.total.toInt()), style = MaterialTheme.typography.bodyMedium)
        val points = f.points?.let {
            val name = if (f.event == EventTypeEnum.S) R.string.aces else R.string.points
            "${stringResource(name)} $it"
        }
        val errors = "${stringResource(R.string.stats_errors)} ${f.errors}"
        Text(listOfNotNull(points, errors).joinToString(" · "), style = MaterialTheme.typography.labelMedium, color = muted)
    }
}
