package io.github.naighes.scout4all

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.AnalysisPeriod
import io.github.naighes.scout4all.core.ClipPick
import io.github.naighes.scout4all.core.DetailStats
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.StatsScope
import io.github.naighes.scout4all.core.TeamClipMatch

/** What the detail picks, as the core reads it. */
private fun pickOf(scope: StatsScope, query: DetailQuery, choice: ClipChoice) =
    ClipPick(scope, query.event, query.filter, choice.evals, choice.zone, choice.target)

/** The actions of a statistic of the match in its video. */
fun matchWatch(teamId: String, matchId: String, scope: StatsScope, onWatch: (VideoRequest.Clips) -> Unit) = Watch { detail, query, choice, clear ->
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val pick = pickOf(scope, query, choice)
    val count by produceState<Int?>(null, pick) {
        value = try {
            app.core { it.matchClipCount(teamId, matchId, pick) }.toInt()
        } catch (_: MobileException) {
            null
        }
    }
    val hasVideo by produceState(false) { value = runCatching { app.core { it.hasVideo(teamId, matchId) } }.getOrDefault(false) }
    val title = clipsTitle(detail, scope, query, choice)
    WatchHeader(
        count = count,
        enabled = hasVideo && (count ?: 0) > 0,
        choice = choice,
        clear = clear,
        lines = listOf(stringResource(if (hasVideo) R.string.clips_pick_hint else R.string.clips_no_video)),
    ) {
        val match = TeamClipMatch(matchId, "", "", (count ?: 0).toUInt(), true)
        onWatch(VideoRequest.Clips(teamId, pick, title, listOf(match)))
    }
}

/** The actions of a statistic of the team over a period, match after match
 * (the ones with a video). */
fun teamWatch(teamId: String, period: AnalysisPeriod, onWatch: (VideoRequest.Clips) -> Unit) = Watch { detail, query, choice, clear ->
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val pick = pickOf(StatsScope.Match, query, choice)
    val matches by produceState<List<TeamClipMatch>?>(null, pick, period) {
        value = try {
            app.core { it.teamClipMatches(teamId, period, pick) }
        } catch (_: MobileException) {
            null
        }
    }
    val watched = matches.orEmpty().filter { it.hasVideo }
    val count = watched.sumOf { it.actions.toInt() }
    val skipped = matches.orEmpty().size - watched.size
    val title = clipsTitle(detail, StatsScope.Match, query, choice)
    WatchHeader(
        count = matches?.let { count },
        enabled = count > 0,
        choice = choice,
        clear = clear,
        lines = listOfNotNull(
            matches?.let { stringResource(R.string.clips_in_matches, count, watched.size) },
            skipped.takeIf { it > 0 }?.let { stringResource(R.string.clips_without_video, it) },
            stringResource(R.string.clips_pick_hint),
        ),
    ) {
        onWatch(VideoRequest.Clips(teamId, pick, title, watched))
    }
}

/** The button to watch the actions, how many, the choice to clear, and
 * what to know. */
@Composable
private fun WatchHeader(
    count: Int?,
    enabled: Boolean,
    choice: ClipChoice,
    clear: () -> Unit,
    lines: List<String>,
    onWatch: () -> Unit,
) = Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Button(onClick = onWatch, enabled = enabled, modifier = Modifier.weight(1f, fill = false).heightIn(min = 48.dp)) {
            Icon(painterResource(R.drawable.ic_video), contentDescription = null, Modifier.size(20.dp))
            Spacer(Modifier.width(8.dp))
            Text(stringResource(R.string.watch_clips) + (count?.let { " · " + stringResource(R.string.clips_count, it) } ?: ""))
        }
        if (!choice.empty) {
            TextButton(onClick = clear) {
                Icon(Icons.Filled.Close, contentDescription = null, Modifier.size(18.dp))
                Spacer(Modifier.width(4.dp))
                Text(stringResource(R.string.clips_clear))
            }
        }
    }
    lines.forEach { Text(it, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
}

/** What the actions picked are, over the video: the fundamental, the set,
 * the evaluations and the zones picked, the player, the rotation, the
 * phase. */
@Composable
private fun clipsTitle(detail: DetailStats, scope: StatsScope, query: DetailQuery, choice: ClipChoice): String {
    val f = detail.fundamental
    val filter = query.filter
    return listOfNotNull(
        detail.fundamentals.find { it.value == query.event }?.name?.replaceFirstChar { it.uppercase() },
        (scope as? StatsScope.Set)?.let { stringResource(R.string.set_n, it.number.toInt()) },
        f?.evals?.filter { it.eval in choice.evals }?.joinToString(" ") { it.symbol }?.takeIf { it.isNotEmpty() },
        choice.zone?.let { stringResource(R.string.clip_zone, it.toString()) },
        choice.target?.let { t -> f?.targets?.flatten()?.find { it.value == t }?.label?.let { stringResource(R.string.clip_zone, it) } },
        filter.player?.let { id -> detail.players.find { it.id == id }?.let { "${it.number} ${it.name}" } },
        filter.rotation?.let { r -> detail.rotations.find { it.value == r }?.name },
        filter.phase?.let { p -> detail.phases.find { it.value == p }?.name?.replaceFirstChar { it.uppercase() } },
    ).joinToString(" · ")
}
