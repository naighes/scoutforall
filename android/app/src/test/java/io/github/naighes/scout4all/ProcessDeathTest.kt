package io.github.naighes.scout4all

import android.os.Bundle
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.isEnabled
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.NewVideoFile
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.Robolectric
import org.robolectric.RobolectricTestRunner

/** Android closes the app in the background, in the middle of a set, and
 * opens it again where it was: the screens from what they saved, the
 * scouting from what the core saved. */
@RunWith(RobolectricTestRunner::class)
class ProcessDeathTest {
    @get:Rule val rule = createEmptyComposeRule()

    private val team = saveNationalTeam()
    private val match = startFirstSet(team)

    private fun touch(text: String, substring: Boolean = false) {
        val touchable = hasText(text, substring = substring) and hasClickAction() and isEnabled()
        rule.waitFor({ "no \"$text\" in ${rule.texts()}" }) { rule.onAllNodes(touchable).fetchSemanticsNodes().size == 1 }
        rule.onNode(touchable).performClick()
    }

    @Test
    fun theSetGoesOnWhereItWasSaved() {
        core { it.acceptTerms() }
        val closed = Robolectric.buildActivity(MainActivity::class.java).setup()
        touch("Italia")
        touch(string(R.string.matches))
        touch("Cortona", substring = true)
        touch(string(R.string.resume_set_n, 1))
        // a serve saved, then a player touched for the next event
        touch("+")
        rule.waitForText(string(R.string.touch_the_player))
        touch("Paola Egonu")
        rule.waitForText("18 · Paola Egonu")
        // the activity saves its state and goes, with its view models: as
        // the process, only the files stay
        val saved = Bundle()
        closed.pause().saveInstanceState(saved).stop().destroy()
        Robolectric.buildActivity(MainActivity::class.java).setup(saved)
        // the scouting again, with the serve; the player touched is lost
        rule.waitForText("serve +", substring = true)
        rule.waitForText(string(R.string.touch_the_player))
    }

    /** The actions picked in the statistics, in their video, when the
     * system closes the app: the video of them again, not the statistics. */
    @Test
    fun theActionsPickedStayInTheVideo() {
        core { it.acceptTerms() }
        serve(team, match, EvalEnum.PERFECT, EvalEnum.PERFECT, EvalEnum.ERROR)
        core { c ->
            c.videoReview(team, match, null).addFiles(listOf(NewVideoFile("content://nowhere/20261025 175500.mp4", "20261025 175500.mp4")), 7200)
        }
        val closed = Robolectric.buildActivity(MainActivity::class.java).setup()
        touch("Italia")
        touch(string(R.string.matches))
        touch("Cortona", substring = true)
        touch(string(R.string.match_stats))
        touch(string(R.string.stats_detail))
        rule.waitForText(string(R.string.clips_pick_hint))
        rule.onNode(hasText("ace", substring = true) and hasClickAction()).performScrollTo().performClick()
        touch(string(R.string.watch_clips) + " · " + string(R.string.clips_count, 2))
        rule.waitForText("Serve · #")
        val saved = Bundle()
        closed.pause().saveInstanceState(saved).stop().destroy()
        Robolectric.buildActivity(MainActivity::class.java).setup(saved)
        rule.waitForText("Serve · #")
        rule.waitForText(string(R.string.in_sequence))
    }
}
