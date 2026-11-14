package io.github.naighes.scout4all

import android.media.MediaMetadataRetriever
import android.net.Uri
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.click
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performSemanticsAction
import androidx.compose.ui.test.performTouchInput
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.naighes.scout4all.core.AnalysisPeriod
import io.github.naighes.scout4all.core.ClipPick
import io.github.naighes.scout4all.core.EvalEnum
import io.github.naighes.scout4all.core.EventTypeEnum
import io.github.naighes.scout4all.core.NewVideoFile
import io.github.naighes.scout4all.core.StatsScope
import io.github.naighes.scout4all.core.VideoSeconds
import io.github.naighes.scout4all.core.stripBars
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.ParameterizedRobolectricTestRunner
import org.robolectric.RobolectricTestRunner
import org.robolectric.shadows.ShadowMediaMetadataRetriever
import org.robolectric.shadows.util.DataSource

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

/** The video of the match read as ten minutes long (the shadow of the
 * reader of Robolectric), the three aces tied a minute apart from the
 * first minute: their clips apart on the strip. */
private fun aceClips(team: String, match: String): VideoRequest.Clips {
    linkVideo(team, match)
    ShadowMediaMetadataRetriever.addMetadata(
        DataSource.toDataSource(app, Uri.parse("content://nowhere/20261025 175500.mp4")),
        MediaMetadataRetriever.METADATA_KEY_DURATION,
        "600000",
    )
    val pick = ClipPick(StatsScope.Match, EventTypeEnum.S, DetailQuery().filter, listOf(EvalEnum.PERFECT), null, null)
    core { c ->
        val review = c.videoReview(team, match, pick)
        review.anchorHere(0u, 60_000u)
        review.correctHere(1u, 120_000u)
        review.correctHere(2u, 180_000u)
    }
    val matches = core { it.teamClipMatches(team, AnalysisPeriod.LastMatches(4u), pick) }
    return VideoRequest.Clips(team, pick, "Serve · #", matches)
}

/** A file of the video linked to the match (`name` tells when it starts,
 * or not), read as ten minutes long by the phone when `readable`. */
private fun videoOf(team: String, match: String, name: String, readable: Boolean) {
    val path = "content://nowhere/$match/$name"
    core { c -> c.videoReview(team, match, null).addFiles(listOf(NewVideoFile(path, name)), 7200) }
    if (readable) {
        ShadowMediaMetadataRetriever.addMetadata(
            DataSource.toDataSource(app, Uri.parse(path)),
            MediaMetadataRetriever.METADATA_KEY_DURATION,
            "600000",
        )
    }
}

/** The aces of a team in five matches: two in the video, tied a minute
 * apart; one with its file not readable, one not lined up (the name of
 * its file doesn't say when it starts), one without a video. */
private fun acesOfFiveMatches(): VideoRequest.Clips {
    val team = saveNationalTeam()
    val pick = ClipPick(StatsScope.Match, EventTypeEnum.S, DetailQuery().filter, listOf(EvalEnum.PERFECT), null, null)
    listOf("Siena" to 3, "Arezzo" to 2, "Perugia" to 1, "Pisa" to 1, "Lucca" to 1).forEach { (opponent, aces) ->
        val match = startFirstSet(team, opponent)
        serve(team, match, *Array(aces) { EvalEnum.PERFECT })
        when (opponent) {
            "Siena", "Arezzo" -> {
                videoOf(team, match, "20261025 175500.mp4", readable = true)
                core { c ->
                    val review = c.videoReview(team, match, pick)
                    review.anchorHere(0u, 60_000u)
                    (1 until aces).forEach { review.correctHere(it.toUInt(), (60_000 * (it + 1)).toULong()) }
                }
            }
            "Perugia" -> videoOf(team, match, "20261025 175500.mp4", readable = false)
            "Pisa" -> videoOf(team, match, "pisa.mp4", readable = true)
        }
    }
    val matches = core { it.teamClipMatches(team, AnalysisPeriod.LastMatches(10u), pick) }
    return VideoRequest.Clips(team, pick, "Serve · #", matches.filter { it.hasVideo }, matches.filter { !it.hasVideo })
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

    /** The events marked as TalkBack says them: the aces points won, the
     * serve error a point lost and a direct error. */
    @Test
    fun theEventsMarkedByPointAndError() {
        val (team, match) = servedMatch()
        linkVideo(team, match)
        rule.setContent { Scout4AllTheme { VideoScreen(VideoRequest.Match(team, match), onBack = {}) } }
        rule.waitForText(string(R.string.video_missing, "20261025 175500.mp4"))
        val won = string(R.string.video_point_won)
        rule.onNodeWithText("1-0, $won").assertExists()
        rule.onNodeWithText("3-0, $won").assertExists()
        rule.onNodeWithText("3-1, ${string(R.string.video_point_lost)}, ${string(R.string.video_direct_error)}").assertExists()
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

    /** The strip of the clips under the bar: one under each ace, the first
     * playing; a touch on the second plays it from its start (the margin
     * before it), the one playing on the strip; between them, the video
     * there. */
    @Test
    fun theStripOfTheClips() {
        val (team, match) = servedMatch()
        val request = aceClips(team, match)
        var model: VideoModel? = null
        rule.setContent {
            Scout4AllTheme {
                VideoScreen(request, onBack = {})
                model = viewModel<VideoModel>()
            }
        }
        rule.waitFor { model?.total == 600_000L }
        val shown = model!!
        rule.waitForIdle()
        val spans = shown.spans
        assertEquals(listOf(55_000uL to 65_000uL, 115_000uL to 125_000uL, 175_000uL to 185_000uL), spans.map { it.fromMs to it.toMs })
        assertEquals(listOf(0u), spans.filter { it.playing }.map { it.event })
        val strip = rule.onNodeWithTag("strip")
        strip.assertExists()
        // the middle of the second clip, in dp across the strip
        strip.performTouchInput {
            val bars = stripBars(spans, 600_000uL, width / density)
            click(Offset((bars[1].from + bars[1].to) / 2 * density, centerY))
        }
        rule.waitForIdle()
        assertEquals(1, shown.selected)
        assertEquals(115_000L, shown.position)
        assertEquals(listOf(1u), shown.spans.filter { it.playing }.map { it.event })
        // between the first two: the video there
        strip.performTouchInput {
            val bars = stripBars(spans, 600_000uL, width / density)
            click(Offset((bars[0].to + bars[1].from) / 2 * density, centerY))
        }
        rule.waitForIdle()
        assertTrue("${shown.position}", shown.position in 70_000L..110_000L)
    }

    /** The video of the aces of the team: summed up before making it, the
     * matches in the video each after its card, the aces counted across
     * them; the others left out, said why. */
    @Test
    fun theVideoOfTheActionsOfATeam() {
        val request = acesOfFiveMatches()
        val titles = TitleTexts(app.getString(R.string.clips_match_of), app.getString(R.string.clips_count))
        val plan = runBlocking { montagePlan(app, request, null, 5u, 5u, titles) }
        assertEquals(5, plan.actions)
        assertEquals(listOf("Arezzo", "Siena"), plan.matches.map { it.opponent }.sorted())
        // one piece an ace (a minute apart), counted across the matches
        assertEquals((1..5).map { it.toUInt() to 5u }, plan.matches.flatMap { m -> m.pieces.map { it.number to it.count } })
        plan.matches.forEachIndexed { i, m ->
            assertEquals(m.opponent, m.title?.first())
            assertEquals(string(R.string.clips_match_of, i + 1, 2) + " · " + string(R.string.clips_count, m.pieces.size), m.title?.last())
        }
        assertEquals(
            setOf("Lucca" to LeftOut.NO_VIDEO, "Perugia" to LeftOut.UNREADABLE, "Pisa" to LeftOut.NOT_LINED_UP),
            plan.leftOut.toSet(),
        )
        // in the screen: summed up, made once asked
        rule.setContent { Scout4AllTheme { VideoScreen(request, onBack = {}) } }
        rule.waitForText(string(R.string.clip_before))
        rule.onNodeWithContentDescription(string(R.string.share_video)).performSemanticsAction(SemanticsActions.OnClick)
        rule.waitForText(string(R.string.clips_in_matches, 5, 2))
        rule.waitForText(string(R.string.left_out_not_lined_up), substring = true)
        rule.onNodeWithText(string(R.string.montage_left_out, ""), substring = true).assertExists()
        rule.onNodeWithText(string(R.string.make_video)).assertIsEnabled()
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

    /** The actions picked, the strip of their clips under the bar. */
    @Test
    fun theActionsPicked() {
        val (team, match) = servedMatch()
        val request = aceClips(team, match)
        rule.setContent { Scout4AllTheme { VideoScreen(request, onBack = {}) } }
        rule.waitForText(string(R.string.clip_before))
        // turned, under the video that a touch shows and hides
        rule.onNodeWithTag("strip", useUnmergedTree = true).assertExists()
        rule.shot("video-clips-$screen")
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true) + rule.inaccessible())
    }
}
