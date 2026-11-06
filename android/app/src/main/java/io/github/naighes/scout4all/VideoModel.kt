package io.github.naighes.scout4all

import android.content.Context
import android.media.MediaMetadataRetriever
import android.net.Uri
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.setValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import androidx.media3.common.MediaItem
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import io.github.naighes.scout4all.core.ClipMargins
import io.github.naighes.scout4all.core.ClipPick
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.NewVideoFile
import io.github.naighes.scout4all.core.TeamClipMatch
import io.github.naighes.scout4all.core.VideoAlignment
import io.github.naighes.scout4all.core.VideoEventRow
import io.github.naighes.scout4all.core.VideoFileRow
import io.github.naighes.scout4all.core.VideoReview
import io.github.naighes.scout4all.core.VideoStanding
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.util.TimeZone

/** What the video screen opens: the whole match, or the actions picked in
 * the statistics, of a match or of the matches of a team one after the
 * other (the ones with a video). */
sealed interface VideoRequest {
    val teamId: String

    data class Match(override val teamId: String, val matchId: String) : VideoRequest

    data class Clips(
        override val teamId: String,
        val pick: ClipPick,
        /** what they are: the fundamental, the filters */
        val title: String,
        /** at least one, each with a video */
        val matches: List<TeamClipMatch>,
    ) : VideoRequest
}

/** The actions to watch, from the statistics to the video screen: a pick
 * isn't a route. Lost with the process: the screen goes back then. */
object ClipRequests {
    var current: VideoRequest.Clips? = null
}

/** How far the buttons go back and forward. */
const val STEP_SECONDS = 5

/** How long before an event the video goes to it, out of clip mode: the
 * serve is seen coming. */
private const val LEAD_MS = 2000L

/** How far a touch shifts the sync. */
const val SHIFT_MS = 500L

val SPEEDS = listOf(0.5f, 1f, 1.5f, 2f)

/** The video of a match and its events (see `VideoReview` in the core):
 * the player across its files, the event seen as it goes, the clips of the
 * actions picked. Kept while the phone turns. */
class VideoModel(private val app: Scout4AllApplication, private val request: VideoRequest) : ViewModel() {
    val player: ExoPlayer = ExoPlayer.Builder(app).build()

    var review by mutableStateOf<VideoReview?>(null)
        private set
    var loading by mutableStateOf(true)
        private set
    /** why the video can't be shown, if it can't */
    var error by mutableStateOf<String?>(null)
        private set
    /** a file of the video not found (moved, deleted, its permission gone) */
    var missing by mutableStateOf<String?>(null)
        private set
    var events by mutableStateOf<List<VideoEventRow>>(emptyList())
        private set
    var files by mutableStateOf<List<VideoFileRow>>(emptyList())
        private set
    var alignment by mutableStateOf<VideoAlignment>(VideoAlignment.Missing)
        private set
    var selected by mutableIntStateOf(0)
        private set
    /** each change asks the list to show the event chosen */
    var scrolls by mutableIntStateOf(0)
        private set
    var position by mutableLongStateOf(0L)
        private set
    var total by mutableLongStateOf(0L)
        private set
    var playing by mutableStateOf(false)
        private set
    var speed by mutableStateOf(1f)
        private set
    /** going to an event plays it and stops a little after */
    var clip by mutableStateOf(request is VideoRequest.Clips)
        private set
    /** with the actions picked, at the end of a clip the next one starts */
    var sequence by mutableStateOf(true)
        private set
    /** the video before the first event shown, to tie the events again */
    var whole by mutableStateOf(false)
        private set
    var margins by mutableStateOf(ClipMargins(5u, 5u, true))
        private set
    /** with the actions of a team, the match shown */
    var at by mutableIntStateOf(0)
        private set
    var standing by mutableStateOf<VideoStanding?>(null)
        private set

    /** the event chosen follows the video: off once chosen by hand, on
     * again when the video is moved or played */
    private var follow = true
    /** where the video went for the event chosen: that one stays chosen
     * until the video passes an event after it (the position read may still
     * be the old one, or a little before) */
    private var goneTo: Long? = null
    /** where the clip playing stops */
    private var clipEnd: Long? = null
    private var firstMs: Long? = null
    private var durations: List<Long> = emptyList()

    val clips: VideoRequest.Clips? get() = request as? VideoRequest.Clips
    val matchId: String get() = when (request) {
        is VideoRequest.Match -> request.matchId
        is VideoRequest.Clips -> request.matches.getOrNull(at)?.matchId.orEmpty()
    }

    init {
        player.addListener(object : Player.Listener {
            override fun onIsPlayingChanged(isPlaying: Boolean) {
                playing = isPlaying
            }
        })
        viewModelScope.launch {
            margins = app.core { it.clipMargins() }
            open(0)
        }
        viewModelScope.launch {
            while (isActive) {
                tick()
                delay(100)
            }
        }
    }

    override fun onCleared() {
        player.release()
    }

    /** The match `index` of the actions of a team, or the match: its review
     * and its files. */
    private suspend fun open(index: Int) {
        loading = true
        error = null
        at = index
        val pick = clips?.pick
        review = try {
            app.core { it.videoReview(request.teamId, matchId, pick) }
        } catch (e: MobileException) {
            error = e.reason
            null
        }
        loadFiles()
        loading = false
        if (clips != null) events.indices.firstOrNull { target(it) != null }?.let { goToEvent(it) }
    }

    /** The files of the video read (how long each is) and played one after
     * the other; the events placed in them. */
    private suspend fun loadFiles() {
        val r = review ?: return
        player.stop()
        player.clearMediaItems()
        missing = null
        durations = emptyList()
        val rows = r.files()
        if (rows.isNotEmpty() && !r.newer()) {
            val read = withContext(Dispatchers.IO) { rows.map { durationOf(app, it.path) } }
            when (val lost = rows.zip(read).firstOrNull { it.second == null }) {
                null -> {
                    durations = read.filterNotNull()
                    app.onCore { r.setDurations(durations.map { it.toULong() }) }
                    player.setMediaItems(rows.map { MediaItem.fromUri(Uri.parse(it.path)) })
                    player.prepare()
                }
                else -> missing = lost.first.name
            }
        }
        refresh()
        seek(start())
    }

    /** What the review shows, read again after a change. */
    private fun refresh() {
        val r = review ?: return
        events = r.events()
        files = r.files()
        alignment = r.alignment()
        firstMs = r.firstMs()
        total = durations.sum()
        selected = selected.coerceIn(0, (events.size - 1).coerceAtLeast(0))
    }

    /** Where the video is, across its files. */
    private fun videoPosition(): Long =
        durations.take(player.currentMediaItemIndex).sum() + player.currentPosition.coerceAtLeast(0)

    private fun tick() {
        val r = review ?: return
        if (durations.isEmpty()) return
        position = videoPosition()
        standing = r.standing(position.toULong())
        reached(position)
    }

    /** The video at `ms`: the event seen chosen; the clip over, the next one
     * played (in sequence) or the video stopped. With the actions picked,
     * the one whose clip is playing stays chosen (the next may start before
     * it ends). */
    private fun reached(ms: Long) {
        val end = clipEnd
        val playingClip = clips != null && end != null
        when {
            end != null && ms >= end -> {
                clipEnd = null
                val next = nextClip()
                when {
                    next != null -> goToEvent(next)
                    nextMatch() != null -> otherMatch(true)
                    else -> player.pause()
                }
            }
            !playingClip -> followVideo(ms)
        }
    }

    private fun followVideo(ms: Long) {
        val r = review ?: return
        val seen = r.current(ms.toULong(), lead())?.toInt() ?: return
        val gone = goneTo
        if (gone != null) {
            if ((target(seen) ?: return) <= gone) return
            goneTo = null
        }
        val sameTime = target(seen) == target(selected)
        if (follow && !sameTime && seen != selected) {
            selected = seen
            scrolls++
        }
    }

    private fun nextClip(): Int? = when {
        clips != null && sequence -> review?.nextClip(selected.toUInt(), lead())?.toInt()
        else -> null
    }

    private fun nextMatch(): Int? = clips?.takeIf { sequence }?.let { c -> (at + 1).takeIf { it < c.matches.size } }

    /** How long before an event the video goes to it: in clip mode, the
     * seconds chosen. */
    fun lead(): Long = if (clip || clips != null) margins.beforeS.toLong() * 1000 else LEAD_MS

    fun target(i: Int): Long? = review?.target(i.toUInt(), lead())?.toLong()

    /** Where the video starts: a little before the first event of the match
     * once lined up (the warm-up left out), unless the whole video is
     * asked for, to tie the events again. */
    private fun start(): Long = when {
        !whole && firstMs != null -> (firstMs!! - lead()).coerceAtLeast(0)
        else -> 0
    }

    fun seek(ms: Long) {
        val r = review ?: return
        if (durations.isEmpty()) return
        val to = ms.coerceIn(start(), total)
        follow = true
        goneTo = null
        // moved by hand: no clip to stop at
        clipEnd = null
        val at = r.locate(to.toULong())
        player.seekTo(at.file.toInt(), at.fromMs.toLong())
        position = to
    }

    fun step(forward: Boolean) = seek(position + (if (forward) 1 else -1) * STEP_SECONDS * 1000L)

    fun playPause() {
        follow = true
        if (player.isPlaying) player.pause() else player.play()
    }

    fun nextSpeed() {
        speed = SPEEDS.firstOrNull { it > speed + 0.01f } ?: SPEEDS.first()
        player.setPlaybackSpeed(speed)
    }

    /** The event `i` chosen, the video a little before it; in clip mode
     * played up to a little after it. */
    fun goToEvent(i: Int) {
        selected = i.coerceIn(0, (events.size - 1).coerceAtLeast(0))
        scrolls++
        val to = target(selected) ?: return
        seek(to)
        goneTo = to
        val offset = events.getOrNull(selected)?.offsetMs ?: return
        if (clip) {
            clipEnd = offset.coerceAtLeast(0) + margins.afterS.toLong() * 1000
            player.play()
        }
    }

    /** An event chosen by hand, to tie it: the list stops following. */
    fun choose(i: Int) {
        selected = i
        follow = false
    }

    fun toggleClip() {
        clip = !clip
        clipEnd = null
    }

    fun toggleSequence() {
        sequence = !sequence
    }

    fun toggleWhole() {
        whole = !whole
        if (position < start()) seek(start())
    }

    fun toggleOverlay() = saveMargins(margins.copy(overlay = !margins.overlay))

    fun margin(before: Boolean, by: Int) = saveMargins(
        when (before) {
            true -> margins.copy(beforeS = (margins.beforeS.toInt() + by).coerceAtLeast(0).toUInt())
            false -> margins.copy(afterS = (margins.afterS.toInt() + by).coerceAtLeast(0).toUInt())
        },
    )

    private fun saveMargins(m: ClipMargins) {
        margins = m
        viewModelScope.launch {
            margins = try {
                app.core { it.saveClipMargins(m) }
            } catch (e: MobileException) {
                error = e.reason
                margins
            }
        }
    }

    /** With the actions of a team, the match after (or before) this one,
     * its first action played. */
    fun otherMatch(after: Boolean) {
        val c = clips ?: return
        val next = if (after) at + 1 else at - 1
        if (next !in c.matches.indices) return
        viewModelScope.launch { open(next) }
    }

    /** A change of the profile, saved by the core: the events lined up
     * again. */
    private fun change(files: Boolean = false, action: (VideoReview) -> Unit) {
        val r = review ?: return
        viewModelScope.launch {
            try {
                app.onCore { action(r) }
                if (files) loadFiles() else refresh()
            } catch (e: MobileException) {
                error = e.reason
            }
        }
    }

    fun anchorHere() = change { it.anchorHere(selected.toUInt(), position.toULong()) }.also { whole = false }

    /** The time of the event chosen written: false when it isn't one. */
    suspend fun anchorWritten(text: String): Boolean {
        val r = review ?: return false
        val done = try {
            app.onCore { r.anchorWritten(selected.toUInt(), text) }
        } catch (e: MobileException) {
            error = e.reason
            false
        }
        if (done) {
            whole = false
            refresh()
            goToEvent(selected)
        }
        return done
    }

    fun writtenTime(): String = review?.writtenTime(selected.toUInt(), position.toULong()).orEmpty()

    fun shift(ms: Long) = change { it.shift(ms) }

    fun automatic() = change { it.automatic() }

    fun addFiles(context: Context, uris: List<Uri>) {
        if (uris.isEmpty()) return
        val added = uris.map { uri ->
            // kept readable after the phone restarts
            runCatching {
                context.contentResolver.takePersistableUriPermission(uri, android.content.Intent.FLAG_GRANT_READ_URI_PERMISSION)
            }
            NewVideoFile(uri.toString(), displayName(context, uri))
        }
        val offset = TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 1000
        change(files = true) { it.addFiles(added, offset) }
    }

    fun moveFile(index: Int, earlier: Boolean) = change(files = true) { it.moveFile(index.toUInt(), earlier) }

    fun removeFile(index: Int) = change(files = true) { it.removeFile(index.toUInt()) }

    /** Work that outlives a dialog: saving the video made. */
    fun viewModelScopeLaunch(block: suspend () -> Unit) {
        viewModelScope.launch { block() }
    }

    fun dismissError() {
        error = null
    }
}

/** How long the video is, in milliseconds: none when it can't be read. */
internal fun durationOf(context: Context, path: String): Long? = try {
    MediaMetadataRetriever().run {
        try {
            setDataSource(context, Uri.parse(path))
            extractMetadata(MediaMetadataRetriever.METADATA_KEY_DURATION)?.toLongOrNull()
        } finally {
            release()
        }
    }
} catch (_: RuntimeException) {
    // not found, not readable, not a video
    null
}

/** The name the user sees of a file chosen: it may tell when it starts. */
internal fun displayName(context: Context, uri: Uri): String =
    context.contentResolver.query(uri, arrayOf(android.provider.OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
        if (c.moveToFirst()) c.getString(0) else null
    } ?: uri.lastPathSegment.orEmpty()
