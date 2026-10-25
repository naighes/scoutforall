package io.github.naighes.scout4all

import android.animation.AnimatorSet
import android.animation.ObjectAnimator
import android.content.Intent
import android.net.Uri
import android.os.Bundle
import android.os.SystemClock
import android.view.View
import android.view.animation.AccelerateInterpolator
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.core.animation.doOnEnd
import androidx.core.content.IntentCompat
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.core.splashscreen.SplashScreenViewProvider
import androidx.lifecycle.viewmodel.initializer
import androidx.lifecycle.viewmodel.viewModelFactory
import androidx.navigation.NavBackStackEntry
import androidx.navigation.NavController
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

class MainActivity : ComponentActivity() {
    // made with the application of the activity: the factory of
    // `viewModels()` keeps the first one of the process
    private val teams: TeamsViewModel by viewModels { viewModelFactory { initializer { TeamsViewModel(application) } } }
    private val terms: TermsViewModel by viewModels { viewModelFactory { initializer { TermsViewModel(application) } } }

    // an archive another app opened with this one, until it's shown
    private val opened = MutableStateFlow<Uri?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        val splash = installSplashScreen()
        super.onCreate(savedInstanceState)
        // again when the user chooses another language for the app
        (application as Scout4AllApplication).followLanguage(resources.configuration.locales[0].language)
        // the logo stays until the teams and the terms are there: no empty
        // screen after it
        splash.setKeepOnScreenCondition {
            teams.state.value == TeamsState.Loading || terms.seen.value == null
        }
        splash.setOnExitAnimationListener(::exitSplash)
        enableEdgeToEdge(
            statusBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
            navigationBarStyle = SystemBarStyle.dark(android.graphics.Color.TRANSPARENT),
        )
        // not again when the activity is recreated, e.g. rotated
        if (savedInstanceState == null) opened.value = intent.archive()
        setContent {
            Scout4AllTheme {
                Navigation(teams, terms, opened) { opened.value = null }
            }
        }
    }

    // one activity: an archive opened while the app runs comes here
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        intent.archive()?.let { opened.value = it }
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

/** The archive of "Open with" (a file manager, WhatsApp) or of the share
 * sheet (Quick Share). */
private fun Intent.archive(): Uri? = when (action) {
    Intent.ACTION_VIEW -> data
    Intent.ACTION_SEND -> IntentCompat.getParcelableExtra(this, Intent.EXTRA_STREAM, Uri::class.java)
    else -> null
}

/** The screens: the terms of use until seen, then teams → team → its form,
 * a player or a match → its form, the start of a set, its scouting, its
 * events or the statistics, and the forms of a new team, player or match
 * (which goes on with its first set); teams → settings → terms, about. */
@Composable
private fun Navigation(teams: TeamsViewModel, terms: TermsViewModel, opened: StateFlow<Uri?>, onOpened: () -> Unit) {
    // under the splash until read
    val seen = terms.seen.collectAsState().value ?: return
    val nav = rememberNavController()
    // over any screen, once the terms are accepted
    val import = rememberImport(teamId = null, onTeam = { nav.navigate("team/$it") }, onMatch = nav::openMatch)
    val archive = opened.collectAsState().value
    LaunchedEffect(archive, seen) {
        if (seen && archive != null) {
            onOpened()
            import(archive)
        }
    }
    NavHost(nav, startDestination = if (seen) "teams" else "terms") {
        composable("terms") {
            TermsScreen(
                onAccept = {
                    terms.accept()
                    nav.navigate("teams") { popUpTo("terms") { inclusive = true } }
                },
                onBack = {},
            )
        }
        composable("teams") {
            TeamsScreen(
                teams,
                onTeam = { id -> nav.navigate("team/$id") },
                onNewTeam = { nav.navigate("new-team") },
                onSettings = { nav.navigate("settings") },
                onMatch = nav::openMatch,
            )
        }
        composable("settings") { entry ->
            SettingsScreen(
                onBack = { nav.back(entry) },
                onTerms = { nav.navigate("settings/terms") },
                onAbout = { nav.navigate("settings/about") },
            )
        }
        composable("settings/terms") { entry ->
            TermsScreen(onAccept = null, onBack = { nav.back(entry) })
        }
        composable("settings/about") { entry ->
            AboutScreen(onBack = { nav.back(entry) })
        }
        composable("new-team") { entry ->
            TeamFormScreen(null, onBack = { nav.back(entry) }, onSaved = { nav.back(entry) })
        }
        composable("team/{id}") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            TeamScreen(
                id,
                onBack = { nav.back(entry) },
                onEdit = { nav.navigate("team/$id/edit") },
                onNewPlayer = { nav.navigate("team/$id/new-player") },
                onPlayer = { player -> nav.navigate("team/$id/player/$player") },
                onNewMatch = { nav.navigate("team/$id/new-match") },
                onMatch = { m -> nav.navigate("team/$id/match/${Uri.encode(m)}") },
            )
        }
        composable("team/{id}/edit") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            TeamFormScreen(id, onBack = { nav.back(entry) }, onSaved = { nav.back(entry) })
        }
        composable("team/{id}/new-player") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            PlayerFormScreen(id, null, onBack = { nav.back(entry) }, onDone = { nav.back(entry) })
        }
        composable("team/{id}/player/{player}") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            val player = entry.arguments?.getString("player").orEmpty()
            PlayerFormScreen(id, player, onBack = { nav.back(entry) }, onDone = { nav.back(entry) })
        }
        composable("team/{id}/new-match") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            MatchFormScreen(id, null, onBack = { nav.back(entry) }, onSaved = { m ->
                // as the TUI, a new match goes on with its first set; back
                // from it, the page of the match
                val match = "team/$id/match/${Uri.encode(m)}"
                nav.navigate(match) { popUpTo("team/{id}/new-match") { inclusive = true } }
                nav.navigate("$match/start-set")
            })
        }
        composable("team/{id}/match/{match}") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            val m = entry.arguments?.getString("match").orEmpty()
            val match = "team/$id/match/${Uri.encode(m)}"
            MatchScreen(
                id,
                m,
                onBack = { nav.back(entry) },
                onEdit = { nav.navigate("$match/edit") },
                onStartSet = { nav.navigate("$match/start-set") },
                onResumeSet = { nav.navigate("$match/scouting") },
                onSet = { set -> nav.navigate("$match/set/$set") },
                onStats = { nav.navigate("$match/stats?set=0") },
            )
        }
        composable("team/{id}/match/{match}/set/{set}") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            val m = entry.arguments?.getString("match").orEmpty()
            val set = entry.arguments?.getString("set")?.toIntOrNull() ?: 1
            SetScreen(
                id,
                m,
                set,
                onBack = { nav.back(entry) },
                onStats = { nav.navigate("team/$id/match/${Uri.encode(m)}/stats?set=$set") },
            )
        }
        composable("team/{id}/match/{match}/stats?set={set}") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            val m = entry.arguments?.getString("match").orEmpty()
            val set = entry.arguments?.getString("set")?.toIntOrNull()?.takeIf { it > 0 }
            MatchStatsScreen(id, m, set, onBack = { nav.back(entry) })
        }
        composable("team/{id}/match/{match}/edit") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            val m = entry.arguments?.getString("match").orEmpty()
            MatchFormScreen(id, m, onBack = { nav.back(entry) }, onSaved = { nav.back(entry) })
        }
        composable("team/{id}/match/{match}/start-set") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            val m = entry.arguments?.getString("match").orEmpty()
            // as the TUI, a set started goes on with its scouting; back from
            // it, the page of the match
            SetStartScreen(id, m, onBack = { nav.back(entry) }, onStarted = {
                nav.navigate("team/$id/match/${Uri.encode(m)}/scouting") {
                    popUpTo("team/{id}/match/{match}/start-set") { inclusive = true }
                }
            })
        }
        composable("team/{id}/match/{match}/scouting") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            val m = entry.arguments?.getString("match").orEmpty()
            ScoutingScreen(id, m, onBack = { nav.back(entry) })
        }
    }
}

/** Back from the screen of `entry`, once: a second touch of the arrow
 * before the screen goes would go back from the one under it too, the
 * teams last, and leave an empty screen. */
private fun NavController.back(entry: NavBackStackEntry) {
    if (currentBackStackEntry == entry) popBackStack()
}

/** A match just imported, over its team. */
private fun NavController.openMatch(teamId: String, matchId: String) {
    navigate("team/$teamId")
    navigate("team/$teamId/match/${Uri.encode(matchId)}")
}
