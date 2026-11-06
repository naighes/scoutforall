package io.github.naighes.scout4all

import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Typeface
import android.net.Uri
import androidx.annotation.OptIn
import androidx.core.content.ContextCompat
import androidx.core.graphics.drawable.toBitmap
import androidx.media3.common.MediaItem
import androidx.media3.common.MimeTypes
import androidx.media3.common.OverlaySettings
import androidx.media3.common.util.UnstableApi
import androidx.media3.effect.BitmapOverlay
import androidx.media3.effect.OverlayEffect
import androidx.media3.effect.Presentation
import androidx.media3.effect.StaticOverlaySettings
import androidx.media3.transformer.Composition
import androidx.media3.transformer.EditedMediaItem
import androidx.media3.transformer.EditedMediaItemSequence
import androidx.media3.transformer.Effects
import androidx.media3.transformer.ExportException
import androidx.media3.transformer.ExportResult
import androidx.media3.transformer.ProgressHolder
import androidx.media3.transformer.Transformer
import io.github.naighes.scout4all.core.MontagePiece
import io.github.naighes.scout4all.core.PhaseEnum
import io.github.naighes.scout4all.core.VideoReview
import io.github.naighes.scout4all.core.VideoStanding
import java.io.File

/** The size of the video made: 720p, as on the desktop. */
private const val WIDTH = 1280
private const val HEIGHT = 720

/** How long a card stays before its piece. */
private const val CARD_MS = 2000L

private const val FRAME_RATE = 30

/** What the montage writes on its cards and over the video, in the
 * language of the app. */
data class MontageTexts(
    val team: String,
    val opponent: String,
    /** "Set %d" */
    val setN: String,
    val sideOut: String,
    val breakPoint: String,
)

/** The actions picked made a single video, to share (as `video::montage`
 * on the desktop): for each piece a card (the action, the player, the set
 * and the score before it, which of how many), then the piece from its
 * files, with how the match stands over it when shown. No sound, as on
 * the desktop. Made by Media3 Transformer, on the encoder of the phone. */
@OptIn(UnstableApi::class)
class Montage(
    private val context: Context,
    private val review: VideoReview,
    private val pieces: List<MontagePiece>,
    private val texts: MontageTexts,
    private val overlay: Boolean,
    private val output: File,
    private val onDone: (Result<File>) -> Unit,
) {
    private var transformer: Transformer? = null
    private val progress = ProgressHolder()

    /** Starts it: on the main thread, where it's asked how far it is. */
    fun start() {
        val paths = review.files().map { it.path }
        val folder = File(context.cacheDir, "montage").apply {
            deleteRecursively()
            mkdirs()
        }
        output.parentFile?.mkdirs()
        output.delete()
        val items = pieces.flatMapIndexed { i, piece ->
            val card = File(folder, "card_$i.png").apply { writeCard(this, piece) }
            listOf(cardItem(card)) + review.spans(piece.fromMs, piece.toMs).map { span ->
                // where this stretch starts in the whole video, for the
                // match over it
                val startMs = piece.fromMs.toLong() +
                    review.spans(piece.fromMs, piece.toMs).takeWhile { it != span }.sumOf { (it.toMs - it.fromMs).toLong() }
                videoItem(paths[span.file.toInt()], span.fromMs.toLong(), span.toMs.toLong(), startMs)
            }
        }
        val sequence = EditedMediaItemSequence.Builder(items).build()
        val composition = Composition.Builder(sequence).build()
        transformer = Transformer.Builder(context)
            .setVideoMimeType(MimeTypes.VIDEO_H264)
            .addListener(object : Transformer.Listener {
                override fun onCompleted(composition: Composition, exportResult: ExportResult) {
                    onDone(Result.success(output))
                }

                override fun onError(composition: Composition, exportResult: ExportResult, exportException: ExportException) {
                    output.delete()
                    onDone(Result.failure(exportException))
                }
            })
            .build()
            .also { it.start(composition, output.path) }
    }

    /** How far it is, 0 to 100. */
    fun percent(): Int = when (transformer?.getProgress(progress)) {
        Transformer.PROGRESS_STATE_AVAILABLE -> progress.progress
        else -> 0
    }

    fun cancel() {
        transformer?.cancel()
        output.delete()
    }

    private fun sized() = Presentation.createForWidthAndHeight(WIDTH, HEIGHT, Presentation.LAYOUT_SCALE_TO_FIT)

    private fun cardItem(card: File): EditedMediaItem = EditedMediaItem.Builder(
        MediaItem.Builder().setUri(Uri.fromFile(card)).setImageDurationMs(CARD_MS).build(),
    )
        .setFrameRate(FRAME_RATE)
        .setEffects(Effects(emptyList(), listOf(sized())))
        .build()

    private fun videoItem(path: String, fromMs: Long, toMs: Long, startMs: Long): EditedMediaItem {
        val item = MediaItem.Builder()
            .setUri(Uri.parse(path))
            .setClippingConfiguration(
                MediaItem.ClippingConfiguration.Builder().setStartPositionMs(fromMs).setEndPositionMs(toMs).build(),
            )
            .build()
        val effects = listOfNotNull(sized(), if (overlay) OverlayEffect(listOf(Board(startMs))) else null)
        return EditedMediaItem.Builder(item)
            .setRemoveAudio(true)
            .setEffects(Effects(emptyList(), effects))
            .build()
    }

    /** The card of a piece, on black: the action, its technique and its
     * symbol in amber, then the player, the set and the score before it,
     * which of how many. */
    private fun writeCard(file: File, piece: MontagePiece) {
        val bitmap = Bitmap.createBitmap(WIDTH, HEIGHT, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        canvas.drawColor(android.graphics.Color.BLACK)
        val e = piece.event.event.event
        val title = listOfNotNull(e.name.replaceFirstChar { it.uppercase() }, e.technique).joinToString(" · ") +
            (e.symbol?.let { "  $it" } ?: "")
        val which = "${piece.number} / ${piece.count}" + if (piece.actions > 1u) "  (+${piece.actions - 1u})" else ""
        val lines = listOfNotNull(
            e.player?.let { "${it.number} ${it.name}" },
            "${texts.setN.format(piece.event.setNumber.toInt())} · ${piece.beforeUs}–${piece.beforeThem}",
            which,
        )
        val big = paint(64f, AMBER, bold = true)
        val small = paint(40f, android.graphics.Color.WHITE, bold = false)
        val height = 64f + 28f + lines.size * 56f
        var y = (HEIGHT - height) / 2 + 64f
        canvas.drawText(fit(title, big, WIDTH - 160f), WIDTH / 2f, y, big)
        y += 28f
        lines.forEach { line ->
            y += 56f
            canvas.drawText(fit(line, small, WIDTH - 160f), WIDTH / 2f, y, small)
        }
        file.outputStream().use { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }
        bitmap.recycle()
    }

    /** How the match stands over the piece, top left on a veil: a line a
     * team (the ball by the one to serve, its name, its sets and points),
     * then the set, the phase in its color, the rotation. The same bitmap
     * while the match stands the same. */
    private inner class Board(private val startMs: Long) : BitmapOverlay() {
        private var firstUs: Long? = null
        private var shown: VideoStanding? = null
        private var bitmap: Bitmap = Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888)
        private val settings = StaticOverlaySettings.Builder()
            .setOverlayFrameAnchor(-1f, 1f)
            .setBackgroundFrameAnchor(-0.96f, 0.93f)
            .build()

        override fun getBitmap(presentationTimeUs: Long): Bitmap {
            // the first frame of the item is the start of its stretch
            val first = firstUs ?: presentationTimeUs.also { firstUs = it }
            val ms = startMs + (presentationTimeUs - first) / 1000
            val st = review.standing(ms.coerceAtLeast(0).toULong())
            if (st != shown || shown == null) {
                shown = st
                bitmap = st?.let { board(it) } ?: Bitmap.createBitmap(1, 1, Bitmap.Config.ARGB_8888)
            }
            return bitmap
        }

        override fun getOverlaySettings(presentationTimeUs: Long): OverlaySettings = settings
    }

    private fun board(st: VideoStanding): Bitmap {
        val name = paint(30f, android.graphics.Color.WHITE, bold = true, center = false)
        val sets = paint(30f, 0xCCFFFFFF.toInt(), bold = false, center = false)
        val points = paint(40f, android.graphics.Color.WHITE, bold = true, center = false)
        val soft = paint(28f, 0xCCFFFFFF.toInt(), bold = false, center = false)
        val serving = st.phase == PhaseEnum.BREAK
        val (color, phase) = when (serving) {
            true -> BREAK to texts.breakPoint
            false -> SIDE_OUT to texts.sideOut
        }
        val names = listOf(texts.team, texts.opponent).map { overlayName(it) }
        val nameWidth = names.maxOf { name.measureText(it) }
        val right = listOf(texts.setN.format(st.setNumber.toInt()), "$phase  ${st.rotation}").maxOf { soft.measureText(it) }
        val ball = 30f
        val width = (16 + ball + 10 + nameWidth + 16 + 50 + 70 + 24 + right + 20).toInt()
        val height = 112
        val bitmap = Bitmap.createBitmap(width, height, Bitmap.Config.ARGB_8888)
        val canvas = Canvas(bitmap)
        canvas.drawRoundRect(RectF(0f, 0f, width.toFloat(), height.toFloat()), 18f, 18f, Paint().apply { this.color = 0x8C000000.toInt() })
        val icon = ContextCompat.getDrawable(context, R.drawable.ic_ball)?.apply { setTint(android.graphics.Color.WHITE) }
            ?.toBitmap(ball.toInt(), ball.toInt())
        listOf(
            Triple(names[0], st.setsUs, st.scoreUs) to serving,
            Triple(names[1], st.setsThem, st.scoreThem) to !serving,
        ).forEachIndexed { i, (team, serves) ->
            val base = 46f + i * 46f
            if (serves) icon?.let { canvas.drawBitmap(it, 16f, base - ball + 4f, null) }
            var x = 16 + ball + 10
            canvas.drawText(team.first, x, base, name)
            x += nameWidth + 16
            canvas.drawText(team.second.toString(), x + 50 - sets.measureText(team.second.toString()), base, sets)
            x += 50
            canvas.drawText(team.third.toString(), x + 70 - points.measureText(team.third.toString()), base, points)
        }
        val divider = 16 + ball + 10 + nameWidth + 16 + 50 + 70 + 12
        canvas.drawRect(divider, 18f, divider + 1.5f, height - 18f, Paint().apply { this.color = 0x59FFFFFF })
        canvas.drawText(texts.setN.format(st.setNumber.toInt()), divider + 12, 46f, soft)
        canvas.drawText(phase, divider + 12, 92f, paint(28f, color, bold = true, center = false))
        val phaseWidth = paint(28f, color, bold = true, center = false).measureText(phase)
        canvas.drawText(st.rotation, divider + 12 + phaseWidth + 12, 92f, paint(28f, android.graphics.Color.WHITE, bold = true, center = false))
        return bitmap
    }

    private fun paint(size: Float, color: Int, bold: Boolean, center: Boolean = true) = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        textSize = size
        this.color = color
        typeface = if (bold) Typeface.DEFAULT_BOLD else Typeface.DEFAULT
        if (center) textAlign = Paint.Align.CENTER
    }

    /** The text cut to fit the width. */
    private fun fit(text: String, paint: Paint, width: Float): String = when {
        paint.measureText(text) <= width -> text
        else -> {
            val count = paint.breakText(text, true, width - paint.measureText("…"), null)
            text.take(count).trimEnd() + "…"
        }
    }

    companion object {
        private val AMBER = 0xFFFCD34D.toInt()
        private val SIDE_OUT = 0xFF38BDF8.toInt()
        private val BREAK = 0xFFFB923C.toInt()
    }
}

/** The name of a team over the video: whole up to a point, then cut, so
 * that the corner stays a corner. */
internal fun overlayName(name: String, room: Int = 22): String = when {
    name.length > room -> name.take(room - 1).trimEnd() + "…"
    else -> name
}
