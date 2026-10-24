package io.github.naighes.scout4all

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import io.github.naighes.scout4all.core.LiveStats
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.StatsScope

private sealed interface MatchStatsState {
    data object Loading : MatchStatsState
    data class Loaded(val stats: LiveStats) : MatchStatsState
    data class Failed(val message: String) : MatchStatsState
}

/** The statistics of a match, the same pages as while scouting: of a set,
 * `set` at first, or of the whole match with no `set`. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun MatchStatsScreen(teamId: String, matchId: String, set: Int?, onBack: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    // the set shown, 0 for the match: saved as a number on rotation
    var shown by rememberSaveable { mutableIntStateOf(set ?: 0) }
    // the pages stay while the next scope is read
    val state by produceState<MatchStatsState>(MatchStatsState.Loading, shown) {
        val scope = if (shown == 0) StatsScope.Match else StatsScope.Set(shown.toUByte())
        value = try {
            MatchStatsState.Loaded(app.core { it.matchStats(teamId, matchId, scope) })
        } catch (e: MobileException) {
            MatchStatsState.Failed(e.reason)
        }
    }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.statistics)) },
                navigationIcon = { BackButton(onBack) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val s = state) {
            MatchStatsState.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is MatchStatsState.Failed -> Centered(modifier) { Text(s.message) }
            is MatchStatsState.Loaded -> Column(modifier) {
                StatsPages(
                    s.stats,
                    onScope = { scope ->
                        shown = when (scope) {
                            is StatsScope.Set -> scope.number.toInt()
                            StatsScope.Match -> 0
                        }
                    },
                    loadDetail = { scope, event, filter ->
                        try {
                            app.core { it.matchDetail(teamId, matchId, scope, event, filter) }
                        } catch (e: MobileException) {
                            null
                        }
                    },
                )
            }
        }
    }
}
