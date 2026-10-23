package io.github.naighes.scout4all

import android.animation.AnimatorSet
import android.animation.ObjectAnimator
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

/** The screens: teams → team → its form, and the form of a new team. */
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
            TeamScreen(id, onBack = { nav.popBackStack() }, onEdit = { nav.navigate("team/$id/edit") })
        }
        composable("team/{id}/edit") { entry ->
            val id = entry.arguments?.getString("id").orEmpty()
            TeamFormScreen(id, onBack = { nav.popBackStack() }, onSaved = { nav.popBackStack() })
        }
    }
}
