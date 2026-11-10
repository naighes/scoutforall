package io.github.naighes.scout4all

import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performSemanticsAction
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.naighes.scout4all.core.AnalysisPeriod
import io.github.naighes.scout4all.core.ClipPick
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.EventTypeEnum
import io.github.naighes.scout4all.core.NewVideoFile
import io.github.naighes.scout4all.core.StatsScope
import io.github.naighes.scout4all.core.VideoSeconds
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.RobolectricTestRunner

/** A match with three aces and a serve error: the actions to watch. */
private fun servedMatch(): Pair<String, String> {
    val team = saveNationalTeam()
    val match = startFirstSet(team)
    serve(team, match, EvalEnum.PERFECT, EvalEnum.PERFECT, EvalEnum.PERFECT, EvalEnum.ERROR)
    return team to match
}

/** A file of the video linked to the match, not there: the phone can't
 * read it. */
private fun linkVideo(team: String, match: String) = core { c ->
    c.videoReview(team, match, null).addFiles(listOf(NewVideoFile("content://nowhere/20261025 175500.mp4", "20261025 175500.mp4")), 7200)
}

/** The video of a match and the actions of its statistics in it (001). */
@RunWith(RobolectricTestRunner::class)
class VideoTest {
    @get:Rule val rule = createComposeRule()

    /** No file yet: what to do, and nothing to play. */
    @Test
    fun aMatchWithoutVideo() {
        val (team, match) = servedMatch()
        rule.setContent { Scout4AllTheme { VideoScreen(VideoRequest.Match(team, match), onBack = {}) } }
        rule.waitForText(string(R.string.video_none))
        rule.onNodeWithText(string(R.string.video_choose)).assertIsEnabled()
    }

    /** A file moved or deleted: said, with the way to choose it again; the
     * events are there all the same, lined up by the name of the file. */
    @Test
    fun aFileNotFound() {
        val (team, match) = servedMatch()
        linkVideo(team, match)
        rule.setContent { Scout4AllTheme { VideoScreen(VideoRequest.Match(team, match), onBack = {}) } }
        rule.waitForText(string(R.string.video_missing, "20261025 175500.mp4"))
        rule.onNodeWithText(string(R.string.video_add_files)).assertIsEnabled()
        // the serves in the list, under their set
        rule.waitForText(string(R.string.set_n, 1))
        rule.onNode(hasText(string(R.string.video_files)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText("1. 20261025 175500.mp4")
        rule.waitForText("17:55:00")
        rule.onNode(hasText(string(R.string.video_sync)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.video_aligned_file, "17:55:00"))
    }

    /** The seconds of the settings: the buttons jump by them (and say
     * so), the video goes that long before an event out of clip mode; in
     * clip mode, the margin before the action. */
    @Test
    fun theSecondsOfTheSettings() {
        val (team, match) = servedMatch()
        linkVideo(team, match)
        core { it.setVideoSeconds(VideoSeconds(12u, 0u)) }
        var model: VideoModel? = null
        rule.setContent {
            Scout4AllTheme {
                VideoScreen(VideoRequest.Match(team, match), onBack = {})
                // the one of the screen, already made
                model = viewModel<VideoModel>()
            }
        }
        rule.waitForText(string(R.string.video_missing, "20261025 175500.mp4"))
        rule.onNodeWithContentDescription(string(R.string.video_back_s, 12)).assertExists()
        rule.onNodeWithContentDescription(string(R.string.video_forward_s, 12)).assertExists()
        val shown = model!!
        assertEquals(0L, shown.lead())
        rule.runOnUiThread { shown.toggleClip() }
        assertEquals(shown.margins.beforeS.toLong() * 1000, shown.lead())
        assertEquals(5_000L, shown.lead())
    }

    /** The detail of the serve: all of them, then the aces touching their
     * evaluation; without a video, nothing to watch; with one, the pick
     * goes to the video, named. */
    @Test
    fun theActionsOfAStatistic() {
        val (team, match) = servedMatch()
        var watched: VideoRequest.Clips? = null
        rule.setContent { Scout4AllTheme { MatchStatsScreen(team, match, null, onBack = {}) { watched = it } } }
        rule.waitForText(string(R.string.stats_summary))
        rule.onNode(hasText(string(R.string.stats_detail)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        val all = string(R.string.watch_clips) + " · " + string(R.string.clips_count, 4)
        rule.waitForText(all)
        rule.waitForText(string(R.string.clips_no_video))
        rule.onNodeWithText(all).assertIsNotEnabled()
        rule.onNodeWithText("ace", substring = true).performScrollTo().performSemanticsAction(SemanticsActions.OnClick)
        val aces = string(R.string.watch_clips) + " · " + string(R.string.clips_count, 3)
        rule.waitForText(aces)
        // back to all of them
        rule.onNodeWithText(string(R.string.clips_clear)).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(all)
        assertEquals(null, watched)
    }

    @Test
    fun theActionsPickedGoToTheVideo() {
        val (team, match) = servedMatch()
        linkVideo(team, match)
        var watched: VideoRequest.Clips? = null
        rule.setContent { Scout4AllTheme { MatchStatsScreen(team, match, null, onBack = {}) { watched = it } } }
        rule.waitForText(string(R.string.stats_summary))
        rule.onNode(hasText(string(R.string.stats_detail)) and hasClickAction()).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.clips_pick_hint))
        rule.onNodeWithText("ace", substring = true).performScrollTo().performSemanticsAction(SemanticsActions.OnClick)
        val aces = string(R.string.watch_clips) + " · " + string(R.string.clips_count, 3)
        rule.waitForText(aces)
        rule.onNodeWithText(aces).assertIsEnabled().performSemanticsAction(SemanticsActions.OnClick)
        rule.waitFor { watched != null }
        val request = watched!!
        assertEquals(listOf(EvalEnum.PERFECT), request.pick.evals)
        assertEquals(EventTypeEnum.S, request.pick.event)
        assertEquals(StatsScope.Match, request.pick.scope)
        assertEquals(listOf(match), request.matches.map { it.matchId })
        assertEquals("Serve · #", request.title)
    }

    /** The actions picked, in the video: only them, the clip mode and the
     * sequence on, the margins of the clips. */
    @Test
    fun theActionsPickedInTheVideo() {
        val (team, match) = servedMatch()
        linkVideo(team, match)
        val pick = ClipPick(StatsScope.Match, EventTypeEnum.S, DetailQuery().filter, listOf(EvalEnum.PERFECT), null, null)
        val matches = core { it.teamClipMatches(team, AnalysisPeriod.LastMatches(4u), pick) }
        assertEquals(listOf(match), matches.map { it.matchId })
        val request = VideoRequest.Clips(team, pick, "Serve · #", matches)
        rule.setContent { Scout4AllTheme { VideoScreen(request, onBack = {}) } }
        rule.waitForText("Serve · #")
        rule.waitForText(string(R.string.in_sequence))
        rule.waitForText(string(R.string.clip_before))
        rule.waitForText("5 s")
        // the three aces, not the error
        assertEquals(rule.texts().toString(), 3, rule.texts().count { it.contains("Serve · F  #") })
    }
}

/** The video screen fits every screen. */
@RunWith(ParameterizedRobolectricTestRunner::class)
class VideoLayoutTest(private val screen: Size) {
    @get:Rule(order = 0) val size = SizeRule(screen)
    @get:Rule(order = 1) val rule = createComposeRule()

    companion object {
        @JvmStatic
        @ParameterizedRobolectricTestRunner.Parameters(name = "{0}")
        fun sizes() = sizes.map { arrayOf(it) }
    }

    @Test
    fun theVideoOfTheMatch() {
        val (team, match) = servedMatch()
        linkVideo(team, match)
        rule.setContent { Scout4AllTheme { VideoScreen(VideoRequest.Match(team, match), onBack = {}) } }
        rule.waitForText(string(R.string.video_missing, "20261025 175500.mp4"))
        rule.shot("video-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
    }
}
