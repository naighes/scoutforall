package io.github.naighes.scout4all

import android.accessibilityservice.AccessibilityService
import android.graphics.Bitmap
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasContentDescription
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.lifecycle.Lifecycle
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import java.io.File
import org.json.JSONObject
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** The screenshots of the Play listing, taken by the app on a phone
 * (`scripts/generate_phone_screenshots.py`): the example matches copied into
 * the app, its language chosen, then the screens reached as the user does,
 * by what they read on them (in the language of the app) and not by where
 * they are on the screen. Each one saved as `files/play-shots/<name>.png`,
 * without the bars of the system and at most twice as tall as wide, as
 * Play wants.
 *
 * The data: one team, a match in progress to resume (the one of the
 * scouting, its statistics) with their serve and our reception to come,
 * finished matches for the analysis. */
@RunWith(AndroidJUnit4::class)
class PlayScreenshots {
    @get:Rule val rule = createAndroidComposeRule<MainActivity>()

    private val instrumentation = InstrumentationRegistry.getInstrumentation()
    private val files = instrumentation.targetContext.filesDir
    private val out = File(files, "play-shots")

    @Test
    fun screens() {
        out.deleteRecursively()
        out.mkdirs()
        try {
            take()
        } catch (e: Throwable) {
            // what was on the screen, to see why
            capture("failed")
            throw e
        }
    }

    private fun take() {
        val resume = hasText(text(R.string.resume)) and hasClickAction()
        waitFor(resume)
        shot("8_home")

        // the team, its analysis: each figure match by match
        touch(hasText(teamName()) and hasClickAction())
        touch(hasText(text(R.string.matches)) and hasClickAction())
        touch(hasText(text(R.string.analysis)) and hasClickAction())
        touch(hasText(text(R.string.stats_trend)) and hasClickAction())
        // not the last month: it moves with the day the shots are taken
        touch(hasText(text(R.string.last_matches, 5)) and hasClickAction())
        touch(hasContentDescription(text(R.string.more_matches)) and hasClickAction())
        waitFor(hasText(text(R.string.last_matches, 6)))
        shot("7_analysis")
        back()
        back()

        // the match in progress
        touch(resume)
        val statistics = hasText(text(R.string.stats_swipe)) and hasClickAction()
        waitFor(statistics)
        shot("1_scouting")

        // its statistics, of the whole match
        touch(statistics)
        touch(hasText(text(R.string.whole_match)) and hasClickAction())
        waitFor(hasText(text(R.string.stats_summary)) and hasClickAction())
        shot("3_live_stats")
        touch(hasText(text(R.string.stats_distribution)) and hasClickAction())
        shot("4_distribution")
        touch(hasText(text(R.string.stats_trend)) and hasClickAction(), scroll = true)
        shot("6_trend")
        touch(hasText(text(R.string.stats_detail)) and hasClickAction(), scroll = true)
        shot("5_detail")
        back()

        // our reception, perfect; the attack of the opposite: where it
        // went, its set, the block, its evaluation to choose
        waitFor(statistics)
        touch(hasText("16") and hasClickAction())
        touch(hasText("#") and hasClickAction())
        touch(hasText("18") and hasClickAction())
        touch(zone("5C"))
        touch(hasText("S") and hasClickAction())
        touch(hasText("2") and hasClickAction())
        shot("2_evaluation")
    }

    private fun text(id: Int, vararg args: Any): String = rule.activity.getString(id, *args)

    /** The name of the only team of the data. */
    private fun teamName(): String {
        val teams = File(files, "scout4all").listFiles().orEmpty()
            .map { File(it, "team.json") }
            .filter { it.exists() }
        check(teams.size == 1) { "${teams.size} teams in the data, one expected" }
        return JSONObject(teams[0].readText()).getString("name")
    }

    /** A cell of the court, by its label (the zone and the cell, "5C"). */
    private fun zone(label: String) = SemanticsMatcher("cell $label") { node ->
        node.config.getOrElse(SemanticsProperties.ContentDescription) { emptyList() }
            .any { it.endsWith(" $label") }
    } and hasClickAction()

    private fun waitFor(matcher: SemanticsMatcher) {
        rule.waitUntil(TIMEOUT) { rule.onAllNodes(matcher).fetchSemanticsNodes().size == 1 }
    }

    /** The only node of `matcher`, once there and enabled; brought into view
     * first with `scroll` (a tab out of the row). */
    private fun touch(matcher: SemanticsMatcher, scroll: Boolean = false) {
        val touchable = matcher and isEnabled()
        waitFor(touchable)
        val node = rule.onNode(touchable)
        if (scroll) node.performScrollTo()
        node.performClick()
        rule.waitForIdle()
    }

    private fun back() {
        instrumentation.uiAutomation.performGlobalAction(AccessibilityService.GLOBAL_ACTION_BACK)
        rule.waitForIdle()
    }

    /** The screen once still, if it's the app's. */
    private fun shot(name: String) {
        rule.waitForIdle()
        // the animations of the screens, the sheet, the charts
        Thread.sleep(SETTLE)
        rule.waitForIdle()
        check(capture(name)) { "$name: the app is not on the screen" }
    }

    /** The screen saved, without the bars, if the app is in front (a sheet
     * or a dialog of its own leaves it resumed): never the launcher or
     * another app. */
    private fun capture(name: String): Boolean {
        var resumed = false
        rule.runOnUiThread { resumed = rule.activity.lifecycle.currentState.isAtLeast(Lifecycle.State.RESUMED) }
        if (!resumed) return false
        val screen = instrumentation.uiAutomation.takeScreenshot() ?: return false
        var bars = WindowInsetsCompat.CONSUMED.getInsets(WindowInsetsCompat.Type.systemBars())
        rule.runOnUiThread {
            ViewCompat.getRootWindowInsets(rule.activity.window.decorView)
                ?.let { bars = it.getInsets(WindowInsetsCompat.Type.systemBars()) }
        }
        val width = screen.width
        val between = screen.height - bars.top - bars.bottom
        // still too tall: the rest from both ends
        val extra = (between - 2 * width).coerceAtLeast(0)
        val cropped = Bitmap.createBitmap(screen, 0, bars.top + extra / 2, width, between - extra)
        File(out, "$name.png").outputStream().use { cropped.compress(Bitmap.CompressFormat.PNG, 100, it) }
        return true
    }

    private companion object {
        const val TIMEOUT = 15_000L
        const val SETTLE = 1_500L
    }
}
