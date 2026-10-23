package io.github.naighes.scout4all

import android.animation.AnimatorSet
import android.animation.ObjectAnimator
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
import androidx.core.animation.doOnEnd
import androidx.core.splashscreen.SplashScreen.Companion.installSplashScreen
import androidx.core.splashscreen.SplashScreenViewProvider
import androidx.navigation.compose.NavHost
import androidx.navigation.compose.composable
import androidx.navigation.compose.rememberNavController

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
                Navigation(teams)
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

/** The screens: teams → team → its form, a player or a match → its form,
 * the start of a set or its scouting, and the forms of a new team, player or match (which
 * goes on with its first set). */
@Composable
private fun Navigation(teams: TeamsViewModel) {
    val nav = rememberNavController()
    NavHost(nav, startDestination = "teams") {
        composable("teams") {
            TeamsScreen(
                teams,
                onTeam = { id -> nav.navigate("team/$id") },
                onNewTeam = { nav.navigate("new-team") },
            )
        }
        composable("new-team") {
            TeamFormScreen(null, onBack = { nav.popBackStack() }, onSaved = { nav.popBackStack() })
        }
        composable("team/{id}") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            TeamScreen(
                id,
                onBack = { nav.popBackStack() },
                onEdit = { nav.navigate("team/$id/edit") },
                onNewPlayer = { nav.navigate("team/$id/new-player") },
                onPlayer = { player -> nav.navigate("team/$id/player/$player") },
                onNewMatch = { nav.navigate("team/$id/new-match") },
                onMatch = { m -> nav.navigate("team/$id/match/${Uri.encode(m)}") },
            )
        }
        composable("team/{id}/edit") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            TeamFormScreen(id, onBack = { nav.popBackStack() }, onSaved = { nav.popBackStack() })
        }
        composable("team/{id}/new-player") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            PlayerFormScreen(id, null, onBack = { nav.popBackStack() }, onDone = { nav.popBackStack() })
        }
        composable("team/{id}/player/{player}") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            val player = entry.arguments?.getString("player").orEmpty()
            PlayerFormScreen(id, player, onBack = { nav.popBackStack() }, onDone = { nav.popBackStack() })
        }
        composable("team/{id}/new-match") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            MatchFormScreen(id, null, onBack = { nav.popBackStack() }, onSaved = { m ->
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
                onBack = { nav.popBackStack() },
                onEdit = { nav.navigate("$match/edit") },
                onStartSet = { nav.navigate("$match/start-set") },
                onResumeSet = { nav.navigate("$match/scouting") },
            )
        }
        composable("team/{id}/match/{match}/edit") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            val m = entry.arguments?.getString("match").orEmpty()
            MatchFormScreen(id, m, onBack = { nav.popBackStack() }, onSaved = { nav.popBackStack() })
        }
        composable("team/{id}/match/{match}/start-set") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            val m = entry.arguments?.getString("match").orEmpty()
            // as the TUI, a set started goes on with its scouting; back from
            // it, the page of the match
            SetStartScreen(id, m, onBack = { nav.popBackStack() }, onStarted = {
                nav.navigate("team/$id/match/${Uri.encode(m)}/scouting") {
                    popUpTo("team/{id}/match/{match}/start-set") { inclusive = true }
                }
            })
        }
        composable("team/{id}/match/{match}/scouting") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            val m = entry.arguments?.getString("match").orEmpty()
            ScoutingScreen(id, m, onBack = { nav.popBackStack() })
        }
    }
}
