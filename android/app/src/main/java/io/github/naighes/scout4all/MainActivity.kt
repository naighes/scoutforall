package io.github.naighes.scout4all

import android.content.Intent
import android.content.pm.ActivityInfo
import android.net.Uri
import android.os.Bundle
import android.os.SystemClock
import androidx.activity.ComponentActivity
import androidx.activity.SystemBarStyle
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.foundation.layout.Box
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.window.DialogProperties
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
import io.github.naighes.scout4all.core.NextSetKind
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow

class MainActivity : ComponentActivity() {
    // made with the application of the activity: the factory of
    // `viewModels()` keeps the first one of the process
    private val teams: TeamsViewModel by viewModels { viewModelFactory { initializer { TeamsViewModel(application) } } }
    private val terms: TermsViewModel by viewModels { viewModelFactory { initializer { TermsViewModel(application) } } }
    private val tutorial: TutorialViewModel by viewModels {
        viewModelFactory { initializer { TutorialViewModel(application as Scout4AllApplication) } }
    }

    // an archive another app opened with this one, until it's shown
    private val opened = MutableStateFlow<Uri?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        val splash = installSplashScreen()
        super.onCreate(savedInstanceState)
        // a phone stays upright: the court of the scouting needs the height;
        // a tablet turns
        if (resources.configuration.smallestScreenWidthDp < TABLET_WIDTH) {
            requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_PORTRAIT
        }
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
                CompositionLocalProvider(LocalTutorial provides tutorial.tutorial) {
                    Box {
                        Navigation(teams, terms, tutorial, opened, onOpened = { opened.value = null })
                        TutorialOverlay(tutorial.tutorial)
                    }
                }
                PanicDialog(application as Scout4AllApplication)
                CrashDialog(application as Scout4AllApplication)
            }
        }
    }

    // one activity: an archive opened while the app runs comes here
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        intent.archive()?.let { opened.value = it }
    }

    /** The name of the app under the ball, then the app, once the turn of
     * the ball is over. */
    private fun exitSplash(provider: SplashScreenViewProvider) {
        val remaining = provider.iconAnimationStartMillis + provider.iconAnimationDurationMillis -
            SystemClock.uptimeMillis()
        splashExit(provider.view, provider.iconView).apply {
            startDelay = remaining.coerceIn(0, 700)
            doOnEnd { provider.remove() }
            start()
        }
    }
}

/** The smallest width of a tablet, in dp. */
private const val TABLET_WIDTH = 600

/** The archive of "Open with" (a file manager, WhatsApp) or of the share
 * sheet (Quick Share). */
private fun Intent.archive(): Uri? = when (action) {
    Intent.ACTION_VIEW -> data
    Intent.ACTION_SEND -> IntentCompat.getParcelableExtra(this, Intent.EXTRA_STREAM, Uri::class.java)
    else -> null
}

/** The screens: the terms of use until seen (then the tutorial), then teams →
 * team → its form, a player or a match → its form, the start of a set, its
 * scouting, its events or the statistics, and the forms of a new team,
 * player or match (which goes on with its first set); teams → settings →
 * terms, about. A match and a set started in the tutorial are its
 * examples. */
@Composable
private fun Navigation(
    teams: TeamsViewModel,
    terms: TermsViewModel,
    tutorials: TutorialViewModel,
    opened: StateFlow<Uri?>,
    onOpened: () -> Unit,
) {
    // under the splash until read
    val seen = terms.seen.collectAsState().value ?: return
    val nav = rememberNavController()
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    // the screens in the journal, by their ids
    DisposableEffect(nav) {
        val listener = NavController.OnDestinationChangedListener { _, destination, arguments ->
            val ids = arguments?.keySet().orEmpty().sorted()
                .mapNotNull { key -> arguments?.getString(key)?.let { "$key=$it" } }
            app.journal.add(listOfNotNull("screen", destination.route, ids.joinToString(" ").ifEmpty { null }).joinToString(" "))
        }
        nav.addOnDestinationChangedListener(listener)
        onDispose { nav.removeOnDestinationChangedListener(listener) }
    }
    // the tutorial, if still to show, once the terms are accepted
    LaunchedEffect(seen) { if (seen) tutorials.startIfPending() }
    val tutorial = LocalTutorial.current?.active == true
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
                onResume = { team, match, next ->
                    // over the match: back goes to it
                    nav.openMatch(team, match)
                    val scouting = if (next is NextSetKind.Resume) "scouting" else "start-set"
                    nav.navigate("team/$team/match/${Uri.encode(match)}/$scouting")
                },
            )
        }
        composable("settings") { entry ->
            SettingsScreen(
                onBack = { nav.back(entry) },
                onTerms = { nav.navigate("settings/terms") },
                onAbout = { nav.navigate("settings/about") },
                onTutorial = { tutorials.startAgain { nav.popBackStack("teams", inclusive = false) } },
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
            MatchFormScreen(id, null, example = tutorial, onBack = { nav.back(entry) }, onSaved = { m ->
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
            SetStartScreen(id, m, example = tutorial, onBack = { nav.back(entry) }, onStarted = {
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

/** Over everything once the core panicked: its state may be broken, the
 * app starts again; before, the user may report it. */
@Composable
private fun PanicDialog(app: Scout4AllApplication) {
    val panic = app.panic.collectAsState().value ?: return
    val context = LocalContext.current
    val title = stringResource(R.string.panic_report_title)
    val body = stringResource(R.string.panic_report_body, panic, journaled(app))
    AlertDialog(
        onDismissRequest = {},
        properties = DialogProperties(dismissOnBackPress = false, dismissOnClickOutside = false),
        title = { Text(stringResource(R.string.panic_title)) },
        text = { Text(stringResource(R.string.panic_text)) },
        dismissButton = {
            TextButton(onClick = { context.report(title, body) }) { Text(stringResource(R.string.report)) }
        },
        confirmButton = {
            TextButton(onClick = app::startAgain) { Text(stringResource(R.string.start_again)) }
        },
    )
}

/** The phone and what was done last: what helps to reproduce a problem. */
private fun journaled(app: Scout4AllApplication) =
    (listOf(device(), "Journal:") + app.journal.lines()).joinToString("\n")

/** At the start after a crash: the report, to send or not; either way not
 * offered again. */
@Composable
private fun CrashDialog(app: Scout4AllApplication) {
    val crash = app.crashed.collectAsState().value ?: return
    val context = LocalContext.current
    val title = stringResource(R.string.crash_report_title)
    val body = stringResource(R.string.crash_report_body, crash)
    AlertDialog(
        onDismissRequest = app::forgetCrash,
        title = { Text(stringResource(R.string.crash_title)) },
        text = { Text(stringResource(R.string.crash_text)) },
        dismissButton = {
            TextButton(onClick = app::forgetCrash) { Text(stringResource(R.string.not_now)) }
        },
        confirmButton = {
            TextButton(onClick = {
                context.report(title, body)
                app.forgetCrash()
            }) { Text(stringResource(R.string.report)) }
        },
    )
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
