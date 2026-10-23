package io.github.naighes.scout4all

import android.animation.AnimatorSet
import android.animation.ObjectAnimator
import android.app.Application
import android.os.Bundle
import android.os.SystemClock
import android.view.View
import android.view.animation.AccelerateInterpolator
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.ListItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.core.animation.doOnEnd
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.core.splashscreen.SplashScreenViewProvider
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Scout4All
import io.github.naighes.scout4all.core.TeamListing
import java.io.File
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class MainActivity : ComponentActivity() {
    private val teams: TeamsViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        val splash = installSplashScreen()
        super.onCreate(savedInstanceState)
        // the logo stays until the teams are there: no empty screen after it
        splash.setKeepOnScreenCondition { teams.state.value == TeamsState.Loading }
        splash.setOnExitAnimationListener(::exitSplash)
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        setContent {
            Scout4AllTheme {
                TeamsScreen(teams.state)
            }
        }
    }

    /** The ball grows and fades with the splash, once its turn is over. */
    private fun exitSplash(provider: SplashScreenViewProvider) {
        val icon: View? = provider.iconView
        val remaining = provider.iconAnimationStartMillis + provider.iconAnimationDurationMillis -
            SystemClock.uptimeMillis()
        val fade = ObjectAnimator.ofFloat(provider.view, View.ALPHA, 1f, 0f)
        val grow = listOfNotNull(icon).flatMap {
            listOf(
                ObjectAnimator.ofFloat(it, View.SCALE_X, 1f, 1.25f),
                ObjectAnimator.ofFloat(it, View.SCALE_Y, 1f, 1.25f),
            )
        }
        AnimatorSet().apply {
            playTogether(grow + fade)
            duration = 300
            startDelay = remaining.coerceIn(0, 700)
            interpolator = AccelerateInterpolator()
            doOnEnd { provider.remove() }
            start()
        }
    }
}

/** The teams, read once off the main thread: the core blocks. */
class TeamsViewModel(application: Application) : AndroidViewModel(application) {
    private val mutableState = MutableStateFlow<TeamsState>(TeamsState.Loading)
    val state: StateFlow<TeamsState> = mutableState

    init {
        // the same layout as ~/.scout4all, in the sandbox of the app
        val dataDir = File(application.filesDir, "scout4all").apply { mkdirs() }
        val language = Locale.getDefault().language
        viewModelScope.launch {
            mutableState.value = withContext(Dispatchers.IO) {
                try {
                    Scout4All(dataDir.path, language).use { TeamsState.Loaded(it.teams()) }
                } catch (e: MobileException) {
                    TeamsState.Failed(e.message ?: "")
                }
            }
        }
    }
}

sealed interface TeamsState {
    data object Loading : TeamsState
    data class Loaded(val teams: List<TeamListing>) : TeamsState
    data class Failed(val message: String) : TeamsState
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TeamsScreen(teams: StateFlow<TeamsState>) {
    val state by teams.collectAsState()
    Scaffold(topBar = { TopAppBar(title = { Text(stringResource(R.string.teams)) }) }) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val s = state) {
            TeamsState.Loading -> Centered(modifier) { CircularProgressIndicator() }
            is TeamsState.Failed -> Centered(modifier) { Text(s.message) }
            is TeamsState.Loaded -> when {
                s.teams.isEmpty() -> Centered(modifier) { Text(stringResource(R.string.no_teams)) }
                else -> LazyColumn(modifier) {
                    items(s.teams) { TeamRow(it) }
                }
            }
        }
    }
}

@Composable
private fun TeamRow(team: TeamListing) = when (team) {
    is TeamListing.Team -> ListItem(
        headlineContent = { Text(team.name) },
        supportingContent = {
            val players = team.players.toInt()
            Text("${team.year} · ${pluralStringResource(R.plurals.players, players, players)}")
        },
    )
    is TeamListing.Unreadable -> ListItem(
        headlineContent = { Text(stringResource(R.string.unreadable_team)) },
        supportingContent = { Text(team.folder) },
    )
}

@Composable
private fun Centered(modifier: Modifier, content: @Composable () -> Unit) =
    Box(modifier, contentAlignment = Alignment.Center) { content() }
