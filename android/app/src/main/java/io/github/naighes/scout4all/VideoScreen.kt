package io.github.naighes.scout4all

import io.github.naighes.scout4all.core.EventTypeEnum

import android.app.Activity
import android.content.ClipData
import android.content.Context
import android.content.ContextWrapper
import android.content.Intent
import android.content.pm.ActivityInfo
import android.content.res.Configuration
import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.annotation.OptIn
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.layout.windowInsetsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowLeft
import androidx.compose.material.icons.filled.KeyboardArrowRight
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material.icons.filled.PlayArrow
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Slider
import androidx.compose.material3.Surface
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.FileProvider
import androidx.core.view.WindowCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.lifecycle.viewmodel.compose.viewModel
import androidx.media3.common.util.UnstableApi
import androidx.media3.ui.AspectRatioFrameLayout
import androidx.media3.ui.PlayerView
import io.github.naighes.scout4all.core.PhaseEnum
import io.github.naighes.scout4all.core.VideoAlignment
import io.github.naighes.scout4all.core.VideoEventRow
import io.github.naighes.scout4all.core.VideoStanding
import io.github.naighes.scout4all.core.montageName
import io.github.naighes.scout4all.core.stripBars
import io.github.naighes.scout4all.core.stripClicked
import io.github.naighes.scout4all.core.videoClock
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.SpanStyle

private enum class VideoTab(val title: Int) {
    Events(R.string.video_events),
    Sync(R.string.video_sync),
    Files(R.string.video_files),
}

/** The video of a match (001), or the actions picked in its statistics:
 * upright the video on top with its commands and the events under it (the
 * sync and the files a tab aside); turned, the video on the whole screen,
 * the events a touch away. Never while scouting: the phone is busy then. */
@kotlin.OptIn(ExperimentalMaterial3Api::class)
@Composable
fun VideoScreen(request: VideoRequest, onBack: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val model = viewModel { VideoModel(app, request) }
    val landscape = LocalConfiguration.current.orientation == Configuration.ORIENTATION_LANDSCAPE
    val context = LocalContext.current
    val choose = rememberLauncherForActivityResult(ActivityResultContracts.OpenMultipleDocuments()) { uris ->
        model.addFiles(context, uris)
    }
    val chooseFiles = { choose.launch(arrayOf("video/*")) }
    var montage by remember { mutableStateOf(false) }
    if (montage) MontageDialog(model) { montage = false }
    model.error?.let { message ->
        AlertDialog(
            onDismissRequest = model::dismissError,
            text = { Text(message) },
            confirmButton = { TextButton(onClick = model::dismissError) { Text(stringResource(android.R.string.ok)) } },
        )
    }
    Turning()
    Immersive(landscape)
    when {
        landscape -> FullVideo(model, chooseFiles, onBack)
        else -> Scaffold(
            topBar = {
                TopAppBar(
                    title = {
                        Text(
                            model.clips?.title ?: (stringResource(R.string.video) + " – " + (model.review?.opponent() ?: "")),
                            Modifier.title(),
                            maxLines = 1,
                            overflow = TextOverflow.Ellipsis,
                        )
                    },
                    navigationIcon = { BackButton(onBack) },
                    actions = {
                        if (model.clips != null && model.events.isNotEmpty() && model.files.isNotEmpty()) {
                            IconButton(onClick = { montage = true }) {
                                Icon(Icons.Filled.Share, contentDescription = stringResource(R.string.share_video))
                            }
                        }
                    },
                )
            },
        ) { padding ->
            Column(Modifier.fillMaxSize().padding(padding)) {
                UprightVideo(model, chooseFiles)
            }
        }
    }
}

/** The screen turns as the phone does (as the user lets it), on a phone
 * too, while the video is shown; upright again once left, not while the
 * activity is made again as it turns. */
@Composable
private fun Turning() {
    val activity = LocalContext.current.activity() ?: return
    val app = activity.application as Scout4AllApplication
    DisposableEffect(activity) {
        app.turning = true
        activity.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_FULL_USER
        onDispose {
            if (!activity.isChangingConfigurations) {
                app.turning = false
                activity.upright()
            }
        }
    }
}

private tailrec fun Context.activity(): Activity? = when (this) {
    is Activity -> this
    is ContextWrapper -> baseContext.activity()
    else -> null
}

/** Turned, the bars of the system hidden: the video on the whole screen. */
@Composable
private fun Immersive(on: Boolean) {
    val activity = LocalContext.current.activity() ?: return
    DisposableEffect(on) {
        val controller = WindowCompat.getInsetsController(activity.window, activity.window.decorView)
        if (on) {
            controller.systemBarsBehavior = WindowInsetsControllerCompat.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE
            controller.hide(WindowInsetsCompat.Type.systemBars())
        }
        onDispose { controller.show(WindowInsetsCompat.Type.systemBars()) }
    }
}

@Composable
private fun ColumnScope.UprightVideo(model: VideoModel, chooseFiles: () -> Unit) {
    when {
        model.loading -> Centered(Modifier.fillMaxSize()) { CircularProgressIndicator() }
        model.review?.newer() == true -> Message(stringResource(R.string.video_newer))
        model.files.isEmpty() && model.clips == null -> NoVideo(chooseFiles)
        else -> {
            Box(Modifier.fillMaxWidth().aspectRatio(16f / 9f).background(Color.Black)) {
                VideoSurface(model)
                if (model.margins.overlay) model.standing?.let { Overlay(it, model, compact = true, Modifier.align(Alignment.TopStart).padding(6.dp)) }
                model.missing?.let { MissingFile(it, chooseFiles) }
            }
            Controls(model)
            Options(model)
            if (model.clips != null) {
                ClipsBar(model)
                EventsList(model, Modifier.weight(1f))
            } else {
                var tab by rememberSaveable { mutableIntStateOf(0) }
                PrimaryTabRow(selectedTabIndex = tab) {
                    VideoTab.entries.forEachIndexed { i, t ->
                        Tab(selected = tab == i, onClick = { tab = i }, text = { Text(stringResource(t.title), maxLines = 1) })
                    }
                }
                when (VideoTab.entries[tab]) {
                    VideoTab.Events -> EventsList(model, Modifier.weight(1f))
                    VideoTab.Sync -> SyncPanel(model, Modifier.weight(1f))
                    VideoTab.Files -> FilesPanel(model, chooseFiles, Modifier.weight(1f))
                }
            }
        }
    }
}

/** Turned: the video on black, the match over it; a touch shows the
 * commands for a while, the list of the events slides from the right. */
@Composable
private fun FullVideo(model: VideoModel, chooseFiles: () -> Unit, onBack: () -> Unit) {
    var shown by remember { mutableStateOf(true) }
    var touches by remember { mutableIntStateOf(0) }
    var list by rememberSaveable { mutableStateOf(false) }
    // the commands go away while playing, a few seconds after a touch
    LaunchedEffect(touches, model.playing, list) {
        if (model.playing && !list) {
            delay(3500)
            shown = false
        }
    }
    BackHandler(list) { list = false }
    Row(Modifier.fillMaxSize().background(Color.Black)) {
        Box(
            Modifier.weight(1f).fillMaxHeight().clickable {
                shown = !shown
                touches++
            },
        ) {
            when {
                model.loading -> CircularProgressIndicator(Modifier.align(Alignment.Center))
                model.files.isEmpty() -> Text(
                    stringResource(R.string.video_none),
                    Modifier.align(Alignment.Center).padding(32.dp),
                    color = Color.White,
                )
                // the match over the corner of the video, not of the black
                // beside it
                else -> Box(Modifier.align(Alignment.Center).aspectRatio(16f / 9f)) {
                    VideoSurface(model)
                    if (model.margins.overlay) model.standing?.let { Overlay(it, model, compact = false, Modifier.align(Alignment.TopStart).padding(12.dp)) }
                    model.missing?.let { MissingFile(it, chooseFiles) }
                }
            }
            if (shown) {
                Row(
                    Modifier.align(Alignment.TopEnd).windowInsetsPadding(WindowInsets.safeDrawing).padding(8.dp),
                    horizontalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    RoundButton(onBack) {
                        Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = stringResource(R.string.back), tint = Color.White)
                    }
                    RoundButton({
                        list = !list
                        touches++
                    }) {
                        Icon(painterResource(R.drawable.ic_events), contentDescription = stringResource(R.string.video_events), tint = Color.White)
                    }
                }
                Surface(
                    Modifier.align(Alignment.BottomCenter).fillMaxWidth().windowInsetsPadding(WindowInsets.safeDrawing),
                    color = Color.Black.copy(alpha = 0.6f),
                    contentColor = Color.White,
                ) {
                    Column(Modifier.padding(horizontal = 8.dp)) {
                        Controls(model, onTouch = { touches++ })
                        if (model.clips != null) ClipsBar(model)
                    }
                }
            }
        }
        if (list) {
            // beside the video, not over all of it
            val width = minOf(380, LocalConfiguration.current.screenWidthDp * 45 / 100).dp
            Surface(Modifier.fillMaxHeight().width(width), color = MaterialTheme.colorScheme.surface) {
                Column(Modifier.windowInsetsPadding(WindowInsets.safeDrawing)) {
                    Options(model)
                    EventsList(model, Modifier.weight(1f))
                }
            }
        }
    }
}

@Composable
private fun RoundButton(onClick: () -> Unit, content: @Composable () -> Unit) = Surface(
    shape = RoundedCornerShape(50),
    color = Color.Black.copy(alpha = 0.5f),
) {
    IconButton(onClick = onClick) { content() }
}

/** The frames of the player, fitted on black. */
@OptIn(UnstableApi::class)
@Composable
private fun BoxScope.VideoSurface(model: VideoModel) = AndroidView(
    factory = { context ->
        PlayerView(context).apply {
            useController = false
            resizeMode = AspectRatioFrameLayout.RESIZE_MODE_FIT
            setShutterBackgroundColor(android.graphics.Color.BLACK)
            setKeepContentOnPlayerReset(true)
        }
    },
    update = { it.player = model.player },
    onRelease = { it.player = null },
    modifier = Modifier.matchParentSize().testTag("video"),
)

@Composable
private fun Message(text: String) = Centered(Modifier.fillMaxSize().padding(24.dp)) {
    Text(text, style = MaterialTheme.typography.bodyLarge)
}

/** No file yet: what to do. */
@Composable
private fun NoVideo(chooseFiles: () -> Unit) = Centered(Modifier.fillMaxSize().padding(24.dp)) {
    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(16.dp)) {
        Icon(painterResource(R.drawable.ic_video), contentDescription = null, Modifier.size(48.dp), tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(stringResource(R.string.video_none), style = MaterialTheme.typography.bodyLarge)
        Button(onClick = chooseFiles, Modifier.heightIn(min = 48.dp)) { Text(stringResource(R.string.video_choose)) }
    }
}

@Composable
private fun BoxScope.MissingFile(name: String, chooseFiles: () -> Unit) = Column(
    Modifier.align(Alignment.Center).padding(16.dp),
    horizontalAlignment = Alignment.CenterHorizontally,
    verticalArrangement = Arrangement.spacedBy(8.dp),
) {
    Text(stringResource(R.string.video_missing, name), color = Color.White)
    OutlinedButton(onClick = chooseFiles) { Text(stringResource(R.string.video_add_files)) }
}

/** The bar of the video, then back, play or pause, forward, the time and
 * the speed. */
@Composable
private fun Controls(model: VideoModel, onTouch: () -> Unit = {}) = Column(Modifier.fillMaxWidth().padding(horizontal = 8.dp)) {
    val total = model.total.coerceAtLeast(1)
    // dragged: the thumb follows the finger, the video goes once let go
    var dragged by remember { mutableStateOf<Float?>(null) }
    Slider(
        value = dragged ?: (model.position.toFloat() / total),
        onValueChange = {
            dragged = it
            onTouch()
        },
        onValueChangeFinished = {
            dragged?.let { model.seek((it * total).toLong()) }
            dragged = null
        },
        enabled = model.files.isNotEmpty() && model.missing == null,
        modifier = Modifier.fillMaxWidth().heightIn(max = 36.dp),
    )
    if (model.clips != null) ClipStrip(model, onTouch)
    Row(verticalAlignment = Alignment.CenterVertically) {
        IconButton(onClick = {
            model.step(false)
            onTouch()
        }) {
            Icon(painterResource(R.drawable.ic_back_seconds), contentDescription = stringResource(R.string.video_back_s, model.seconds.stepS.toInt()))
        }
        IconButton(onClick = {
            model.playPause()
            onTouch()
        }) {
            when (model.playing) {
                true -> Icon(painterResource(R.drawable.ic_pause), contentDescription = stringResource(R.string.video_pause))
                false -> Icon(Icons.Filled.PlayArrow, contentDescription = stringResource(R.string.video_play))
            }
        }
        IconButton(onClick = {
            model.step(true)
            onTouch()
        }) {
            Icon(painterResource(R.drawable.ic_forward_seconds), contentDescription = stringResource(R.string.video_forward_s, model.seconds.stepS.toInt()))
        }
        Text(
            "${videoClock(model.position.toULong())} / ${videoClock(model.total.toULong())}",
            Modifier.weight(1f).padding(horizontal = 4.dp),
            style = MaterialTheme.typography.labelMedium,
            maxLines = 1,
        )
        TextButton(
            onClick = {
                model.nextSpeed()
                onTouch()
            },
            Modifier.readAs(stringResource(R.string.video_speed) + " " + speedText(model.speed)),
        ) { Text(speedText(model.speed)) }
    }
}

/** How far from a clip a touch on the strip still takes it, in dp: the
 * `TOUCH_REACH` of the core. */
private const val TOUCH_REACH = 12f

/** The track of the slider starts half its handle (4 dp) from its edges:
 * the strip under it the same, a clip under its time on the bar. */
private val TRACK_INSET = 2.dp

/** With the actions picked, their clips under the bar of the video, where
 * they are in it: red, the one playing taller in the primary colour. A
 * touch on a clip (or near it) plays it from its start, between them the
 * video goes there (`stripBars`, `stripClicked` in the core, the same as on
 * the desktop: widths in dp). TalkBack has the list of the actions. */
@Composable
private fun ClipStrip(model: VideoModel, onTouch: () -> Unit) {
    val spans = model.spans
    val total = model.total
    if (spans.isEmpty() || total <= 0) return
    val clipColor = MaterialTheme.colorScheme.error
    val playingColor = MaterialTheme.colorScheme.primary
    val lineColor = LocalContentColor.current.copy(alpha = 0.25f)
    Canvas(
        Modifier.fillMaxWidth().height(STRIP_HEIGHT).padding(horizontal = TRACK_INSET).testTag("strip")
            .pointerInput(spans, total) {
                detectTapGestures { at ->
                    onTouch()
                    model.strip(stripClicked(spans, total.toULong(), size.width.toDp().value, at.x.toDp().value, TOUCH_REACH))
                }
            },
    ) {
        val line = 2.dp.toPx()
        drawRect(lineColor, Offset(0f, (size.height - line) / 2), Size(size.width, line))
        val bars = stripBars(spans, total.toULong(), size.width.toDp().value)
        // the one playing drawn last, over the others
        bars.zip(spans).sortedBy { it.second.playing }.forEach { (bar, sp) ->
            val height = (if (sp.playing) 12.dp else 6.dp).toPx()
            drawRoundRect(
                if (sp.playing) playingColor else clipColor,
                Offset(bar.from.dp.toPx(), (size.height - height) / 2),
                Size((bar.to - bar.from).dp.toPx(), height),
                CornerRadius(2.dp.toPx()),
            )
        }
    }
}

/** Tall enough for a finger, not more: the video above it matters more. */
private val STRIP_HEIGHT = 24.dp

private fun speedText(speed: Float) = (if (speed % 1f == 0f) speed.toInt().toString() else speed.toString()) + "×"

/** The modes: clip, the match over the video, and in sequence with the
 * actions picked. */
@Composable
private fun Options(model: VideoModel) = Row(
    Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 12.dp),
    horizontalArrangement = Arrangement.spacedBy(8.dp),
) {
    FilterChip(
        selected = model.clip,
        onClick = model::toggleClip,
        label = { Text(stringResource(R.string.video_clip)) },
        modifier = Modifier.readAs(stringResource(R.string.video_clip) + ". " + stringResource(R.string.video_clip_explained, model.margins.afterS.toInt())),
    )
    FilterChip(
        selected = model.margins.overlay,
        onClick = model::toggleOverlay,
        label = { Text(stringResource(R.string.video_overlay)) },
    )
    if (model.clips != null) {
        FilterChip(
            selected = model.sequence,
            onClick = model::toggleSequence,
            label = { Text(stringResource(R.string.in_sequence)) },
        )
    }
}

/** With the actions picked: the seconds of each clip before and after its
 * action and, with the matches of a team, which match. */
@Composable
private fun ClipsBar(model: VideoModel) = Column(Modifier.fillMaxWidth().padding(horizontal = 12.dp)) {
    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.horizontalScroll(rememberScrollState())) {
        Margin(stringResource(R.string.clip_before), model.margins.beforeS.toInt()) { model.margin(true, it) }
        Spacer(Modifier.width(12.dp))
        Margin(stringResource(R.string.clip_after), model.margins.afterS.toInt()) { model.margin(false, it) }
    }
    val c = model.clips ?: return@Column
    if (c.matches.size > 1) {
        val m = c.matches.getOrNull(model.at)
        Row(verticalAlignment = Alignment.CenterVertically) {
            IconButton(onClick = { model.otherMatch(false) }, enabled = model.at > 0) {
                Icon(Icons.Filled.KeyboardArrowLeft, contentDescription = stringResource(R.string.previous_match))
            }
            Text(
                stringResource(R.string.clips_match_of, model.at + 1, c.matches.size) + (m?.let { " · ${it.opponent} · ${shortDay(it.day)}" } ?: ""),
                Modifier.weight(1f),
                style = MaterialTheme.typography.labelLarge,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            IconButton(onClick = { model.otherMatch(true) }, enabled = model.at < c.matches.size - 1) {
                Icon(Icons.Filled.KeyboardArrowRight, contentDescription = stringResource(R.string.next_match))
            }
        }
    }
}

@Composable
private fun Margin(label: String, seconds: Int, onStep: (Int) -> Unit) = Row(verticalAlignment = Alignment.CenterVertically) {
    Text(label, style = MaterialTheme.typography.labelLarge)
    IconButton(onClick = { onStep(-1) }) {
        Icon(Icons.Filled.KeyboardArrowDown, contentDescription = stringResource(R.string.video_less) + " " + label)
    }
    Text("$seconds s", style = MaterialTheme.typography.labelLarge, fontWeight = FontWeight.Bold)
    IconButton(onClick = { onStep(1) }) {
        Icon(Icons.Filled.KeyboardArrowUp, contentDescription = stringResource(R.string.video_more) + " " + label)
    }
}

/** The events, set by set, each at its time in the video; the one seen (or
 * chosen) lit and kept in sight. A touch goes to it. */
@Composable
private fun EventsList(model: VideoModel, modifier: Modifier) {
    val events = model.events
    val list = rememberLazyListState()
    LaunchedEffect(model.scrolls) {
        if (events.isNotEmpty()) list.animateScrollToItem((model.selected - 2).coerceAtLeast(0))
    }
    if (events.isEmpty()) {
        Centered(modifier.fillMaxWidth()) {
            Text(stringResource(R.string.clips_none), color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        return
    }
    LazyColumn(modifier.fillMaxWidth(), state = list) {
        itemsIndexed(events, key = { _, e -> "${e.setNumber}-${e.event.index}" }) { i, e ->
            val newSet = i == 0 || events[i - 1].setNumber != e.setNumber
            if (newSet) {
                Text(
                    stringResource(R.string.set_n, e.setNumber.toInt()),
                    Modifier.fillMaxWidth().padding(start = 16.dp, top = 8.dp, bottom = 2.dp).title(),
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.primary,
                )
            }
            // the score before it, from 0-0 at each set
            val before = events.getOrNull(i - 1)?.takeIf { !newSet }?.event
            EventRow(e, eventMark(e, before?.scoreUs ?: 0u, before?.scoreThem ?: 0u), i == model.selected, model.total) {
                model.goToEvent(i)
            }
        }
    }
}

/** What an event was for the rally, as on the desktop: of the opponent
 * (their point, their error), the point it ended (won or lost, none), a
 * direct error (ours: =, a fault; theirs). */
internal data class EventMark(val theirs: Boolean, val point: Boolean?, val error: Boolean)

internal fun eventMark(e: VideoEventRow, us: UByte, them: UByte): EventMark {
    val event = e.event.event.event
    return EventMark(
        theirs = event == EventTypeEnum.OS || event == EventTypeEnum.OE,
        point = when {
            e.event.scoreUs > us -> true
            e.event.scoreThem > them -> false
            else -> null
        },
        error = event == EventTypeEnum.F || event == EventTypeEnum.OE || e.event.event.symbol == "=",
    )
}

/** An event: a bar of its team on the left (ours light blue, theirs pink),
 * its symbol red on a direct error, its score green or red on the point it
 * ended; TalkBack says which. */
@Composable
private fun EventRow(e: VideoEventRow, mark: EventMark, chosen: Boolean, total: Long, onClick: () -> Unit) {
    val muted = !e.inside || !e.event.editable
    val color = if (muted) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface
    val s = e.event.event
    val name = s.name.replaceFirstChar { it.uppercase() } + (s.technique?.let { " · $it" } ?: "")
    Row(
        Modifier.fillMaxWidth()
            .background(if (chosen) MaterialTheme.colorScheme.surfaceVariant else Color.Transparent)
            .clickable(onClick = onClick)
            .heightIn(min = 44.dp)
            .padding(horizontal = 16.dp, vertical = 6.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Box(
            Modifier.size(width = 4.dp, height = 32.dp)
                .background(if (mark.theirs) TeamThem else TeamUs, RoundedCornerShape(2.dp)),
        )
        Text(
            eventTime(e.offsetMs, total),
            Modifier.width(64.dp),
            style = MaterialTheme.typography.labelMedium.copy(fontFamily = FontFamily.Monospace),
            color = if (chosen) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Column(Modifier.weight(1f)) {
            val symbol = s.symbol ?: "✕".takeIf { mark.error }
            Text(
                buildAnnotatedString {
                    append(name)
                    symbol?.let {
                        append("  ")
                        withStyle(SpanStyle(color = if (mark.error) Lost else color, fontWeight = FontWeight.Bold)) { append(it) }
                    }
                },
                style = MaterialTheme.typography.bodyMedium,
                fontWeight = if (chosen) FontWeight.Bold else null,
                color = color,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            s.player?.let {
                Text(
                    "${it.number} ${it.name}",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (e.anchored) {
            Text("⚓", Modifier.readAs(stringResource(R.string.video_anchored)))
        } else if (e.correctedMs != null) {
            Text("✎", Modifier.readAs(stringResource(R.string.video_corrected)))
        }
        val said = listOfNotNull(
            when (mark.point) {
                true -> stringResource(R.string.video_point_won)
                false -> stringResource(R.string.video_point_lost)
                null -> null
            },
            stringResource(R.string.video_direct_error).takeIf { mark.error },
        ).joinToString(", ")
        val score = "${e.event.scoreUs}-${e.event.scoreThem}"
        Text(
            score,
            Modifier
                .background(
                    when (mark.point) {
                        true -> Won
                        false -> Lost
                        null -> Color.Transparent
                    },
                    RoundedCornerShape(4.dp),
                )
                .padding(horizontal = 6.dp, vertical = 1.dp)
                .then(if (said.isEmpty()) Modifier else Modifier.readAs("$score, $said")),
            style = MaterialTheme.typography.labelLarge,
            fontWeight = if (mark.point != null) FontWeight.Bold else null,
            color = if (mark.point != null) MaterialTheme.colorScheme.surface else color,
        )
    }
}

/** How far an event was moved, signed: `+3.0 s`, `−9.0 s`. */
internal fun signedSeconds(ms: Long): String =
    (if (ms < 0) "−" else "+") + String.format(java.util.Locale.getDefault(), "%.1f s", kotlin.math.abs(ms) / 1000.0)

/** The time of the event in the video, `–` when not lined up; `<` before
 * the start, `>` past the end. */
internal fun eventTime(offsetMs: Long?, totalMs: Long): String = when {
    offsetMs == null -> "–"
    offsetMs < 0 -> "<" + videoClock((-offsetMs).toULong())
    totalMs > 0 && offsetMs > totalMs -> ">" + videoClock(totalMs.toULong())
    else -> videoClock(offsetMs.toULong())
}

/** How the events line up with the video, and the commands to tie them by
 * hand: the event chosen where the video is, or at a time written; all of
 * them shifted; back to automatic; the video before the first event. */
@Composable
private fun SyncPanel(model: VideoModel, modifier: Modifier) = Column(
    modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(16.dp),
    verticalArrangement = Arrangement.spacedBy(12.dp),
) {
    val text = when (val a = model.alignment) {
        VideoAlignment.Missing -> stringResource(R.string.video_not_aligned)
        VideoAlignment.DataVolley -> stringResource(R.string.video_aligned_dvw)
        is VideoAlignment.FileName -> stringResource(R.string.video_aligned_file, a.start)
        VideoAlignment.Chosen -> stringResource(R.string.video_aligned_chosen)
    }
    Text(text, style = MaterialTheme.typography.bodyMedium)
    val outside = model.events.count { it.offsetMs != null && !it.inside }
    if (outside > 0) {
        Text(stringResource(R.string.video_outside, outside), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
    val corrections = model.events.count { it.correctedMs != null }
    if (corrections > 0) {
        Text(stringResource(R.string.video_corrections, corrections), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
    if (model.alignment == VideoAlignment.Chosen || corrections > 0) {
        OutlinedButton(onClick = model::automatic, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) { Text(stringResource(R.string.video_automatic)) }
        Explained(R.string.video_automatic_explained)
    }
    val ready = model.files.isNotEmpty() && model.events.isNotEmpty() && model.missing == null
    val lined = model.alignment != VideoAlignment.Missing
    // once the events line up, a time written moves only its event
    val placed = ready && model.timeOnlyThis()
    var writing by remember { mutableStateOf<String?>(null) }
    val writeTime = @Composable {
        OutlinedButton(onClick = { writing = model.writtenTime() }, enabled = ready, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
            Text(stringResource(R.string.video_anchor_time))
        }
    }
    writing?.let { TimeDialog(model, it) { writing = null } }
    // the whole match: the video tied to the event chosen, all shifted, the
    // video before the first event to find it again
    Text(stringResource(R.string.video_sync_whole), style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
    Button(onClick = model::anchorHere, enabled = ready, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
        Text(stringResource(R.string.video_anchor_here))
    }
    Explained(R.string.video_anchor_here_explained)
    if (!placed) writeTime()
    if (lined) {
        Text(stringResource(R.string.video_shift), style = MaterialTheme.typography.bodyMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            OutlinedButton(onClick = { model.shift(-SHIFT_MS) }, modifier = Modifier.weight(1f).heightIn(min = 48.dp).readAs(stringResource(R.string.video_earlier_half))) {
                Text("−0,5 s")
            }
            OutlinedButton(onClick = { model.shift(SHIFT_MS) }, modifier = Modifier.weight(1f).heightIn(min = 48.dp).readAs(stringResource(R.string.video_later_half))) {
                Text("+0,5 s")
            }
        }
        Explained(R.string.video_shift_explained)
        FilterChip(
            selected = model.whole,
            onClick = model::toggleWhole,
            label = { Text(stringResource(R.string.video_redo_anchor)) },
        )
        Explained(R.string.video_redo_anchor_explained)
    }
    // only the event chosen: put at the moment of the video or at a time
    // written, and back
    Text(stringResource(R.string.video_sync_event), style = MaterialTheme.typography.titleSmall, modifier = Modifier.padding(top = 8.dp))
    model.events.getOrNull(model.selected)?.let { e ->
        Surface(shape = MaterialTheme.shapes.medium, color = MaterialTheme.colorScheme.surfaceVariant) {
            Row(Modifier.fillMaxWidth().padding(12.dp), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(eventTime(e.offsetMs, model.total), style = MaterialTheme.typography.labelLarge, color = MaterialTheme.colorScheme.primary)
                Text(
                    stringResource(R.string.set_n, e.setNumber.toInt()) + " · " +
                        e.event.event.name.replaceFirstChar { it.uppercase() } +
                        (e.event.event.player?.let { " · ${it.number} ${it.name}" } ?: "") +
                        " · ${e.event.scoreUs}-${e.event.scoreThem}",
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
        }
    }
    if (placed) {
        OutlinedButton(onClick = model::correctHere, modifier = Modifier.fillMaxWidth().heightIn(min = 48.dp)) {
            Text(stringResource(R.string.video_correct_here))
        }
        writeTime()
        Explained(R.string.video_correct_here_explained)
    } else {
        Explained(R.string.video_sync_event_unplaced)
    }
    model.events.getOrNull(model.selected)?.correctedMs?.let { by ->
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text(
                stringResource(R.string.video_corrected_by, signedSeconds(by)),
                Modifier.weight(1f),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            TextButton(onClick = model::uncorrect, modifier = Modifier.heightIn(min = 48.dp)) {
                Text(stringResource(R.string.video_uncorrect))
            }
        }
    }
}

/** A line saying what the command above does. */
@Composable
private fun Explained(id: Int) =
    Text(stringResource(id), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)

/** The time of the event chosen, written: the one there selected, the
 * keyboard open, so that what is typed takes its place. */
@Composable
private fun TimeDialog(model: VideoModel, initial: String, onDismiss: () -> Unit) {
    var field by remember { mutableStateOf(TextFieldValue(initial, TextRange(0, initial.length))) }
    val text = field.text
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    var invalid by remember { mutableStateOf(false) }
    val onlyThis = remember { model.timeOnlyThis() }
    val scope = rememberCoroutineScope()
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(stringResource(R.string.video_anchor_time)) },
        text = {
            OutlinedTextField(
                value = field,
                onValueChange = {
                    if (it.text != field.text) invalid = false
                    field = it
                },
                modifier = Modifier.focusRequester(focus),
                singleLine = true,
                isError = invalid,
                supportingText = {
                    Text(
                        stringResource(
                            when {
                                invalid -> R.string.video_invalid_time
                                onlyThis -> R.string.video_time_only_this
                                else -> R.string.video_time_aligns_all
                            },
                        ),
                    )
                },
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Text),
            )
        },
        confirmButton = {
            TextButton(onClick = {
                scope.launch {
                    if (model.anchorWritten(text)) onDismiss() else invalid = true
                }
            }) { Text(stringResource(android.R.string.ok)) }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(android.R.string.cancel)) } },
    )
}

/** The files of the video, in the order played: when each starts, how long
 * it is, moved or taken out; more added. */
@Composable
private fun FilesPanel(model: VideoModel, chooseFiles: () -> Unit, modifier: Modifier) = Column(
    modifier.fillMaxWidth().verticalScroll(rememberScrollState()).padding(vertical = 8.dp),
) {
    model.files.forEachIndexed { i, f ->
        Row(
            Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f)) {
                Text("${i + 1}. ${f.name}", style = MaterialTheme.typography.bodyMedium, maxLines = 2, overflow = TextOverflow.Ellipsis)
                Text(
                    listOfNotNull(f.start, f.durationMs?.let { videoClock(it) }).joinToString(" · ").ifEmpty { "–" },
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            IconButton(onClick = { model.moveFile(i, true) }, enabled = i > 0) {
                Icon(Icons.Filled.KeyboardArrowUp, contentDescription = stringResource(R.string.video_move_earlier))
            }
            IconButton(onClick = { model.moveFile(i, false) }, enabled = i < model.files.size - 1) {
                Icon(Icons.Filled.KeyboardArrowDown, contentDescription = stringResource(R.string.video_move_later))
            }
            IconButton(onClick = { model.removeFile(i) }) {
                Icon(Icons.Filled.Close, contentDescription = stringResource(R.string.video_remove_file))
            }
        }
        HorizontalDivider()
    }
    OutlinedButton(onClick = chooseFiles, modifier = Modifier.fillMaxWidth().padding(16.dp).heightIn(min = 48.dp)) {
        Icon(Icons.Filled.Add, contentDescription = null)
        Spacer(Modifier.width(8.dp))
        Text(stringResource(R.string.video_add_files))
    }
}

/** How the match stands, over the corner of the video, on a veil: a line a
 * team, ours first, as on TV (the ball by the one to serve, its name, the
 * sets and the points), then the set, the phase in its color and the
 * rotation. */
@Composable
private fun Overlay(st: VideoStanding, model: VideoModel, compact: Boolean, modifier: Modifier) {
    val review = model.review ?: return
    // upright, over a small video, shorter
    val room = if (compact) 14 else 22
    val names = remember(review, room) { listOf(overlayName(review.team(), room), overlayName(review.opponent(), room)) }
    val serving = st.phase == PhaseEnum.BREAK
    val (color, phase) = when (serving) {
        true -> BreakPoint to stringResource(R.string.break_phase)
        false -> SideOut to stringResource(R.string.side_out_phase)
    }
    val body = if (compact) 10.sp else 15.sp
    val points = if (compact) 12.sp else 20.sp
    Row(
        modifier.clip(RoundedCornerShape(8.dp)).background(Color.Black.copy(alpha = 0.55f)).padding(horizontal = 8.dp, vertical = 4.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Column {
            listOf(Triple(names[0], st.setsUs, st.scoreUs) to serving, Triple(names[1], st.setsThem, st.scoreThem) to !serving).forEach { (team, serves) ->
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                    Box(Modifier.size(if (compact) 10.dp else 14.dp)) {
                        if (serves) Icon(painterResource(R.drawable.ic_ball), contentDescription = null, tint = Color.White)
                    }
                    Text(team.first, fontSize = body, fontWeight = FontWeight.Bold, color = Color.White, maxLines = 1)
                    Text(team.second.toString(), fontSize = body, color = Color.White.copy(alpha = 0.8f))
                    Text(team.third.toString(), fontSize = points, fontWeight = FontWeight.Bold, color = Color.White)
                }
            }
        }
        Box(Modifier.width(1.dp).heightIn(min = if (compact) 28.dp else 40.dp).background(Color.White.copy(alpha = 0.35f)))
        Column {
            Text(stringResource(R.string.set_n, st.setNumber.toInt()), fontSize = body, color = Color.White.copy(alpha = 0.8f))
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp), verticalAlignment = Alignment.CenterVertically) {
                Text(phase, fontSize = body, fontWeight = FontWeight.Bold, color = color)
                Text(st.rotation, fontSize = body, fontWeight = FontWeight.Bold, color = Color.White)
            }
        }
    }
}

/** The video of the actions picked being made, then shared or saved. Of
 * the matches of a team, summed up first: how many actions of how many
 * matches, the ones left out and why; made once asked. */
@Composable
private fun MontageDialog(model: VideoModel, onDismiss: () -> Unit) {
    val context = LocalContext.current
    val app = context.applicationContext as Scout4AllApplication
    val review = model.review ?: return
    val request = model.clips ?: return
    val texts = MontageTexts(
        setN = stringResource(R.string.set_n),
        sideOut = stringResource(R.string.side_out_phase),
        breakPoint = stringResource(R.string.break_phase),
    )
    val titles = TitleTexts(stringResource(R.string.clips_match_of), stringResource(R.string.clips_count))
    val several = request.matches.size + request.leftOut.size > 1
    val plan by produceState<MontagePlan?>(null) {
        value = montagePlan(app, request, model.matchId to review, model.margins.beforeS, model.margins.afterS, titles)
    }
    // a single match: made at once
    var asked by remember { mutableStateOf(!several) }
    val output = remember { File(File(context.cacheDir, "exports"), montageName(request.title) + ".mp4") }
    var made by remember { mutableStateOf<File?>(null) }
    var failed by remember { mutableStateOf<String?>(null) }
    var percent by remember { mutableFloatStateOf(0f) }
    val montage = remember(plan, asked) {
        plan?.takeIf { asked && it.actions > 0 }?.let { p ->
            Montage(context, p.matches, texts, model.margins.overlay, output) { result ->
                result.fold({ made = it }, { failed = it.localizedMessage ?: "" })
            }
        }
    }
    DisposableEffect(montage) {
        montage?.start()
        onDispose { if (made == null) montage?.cancel() }
    }
    LaunchedEffect(montage) {
        while (montage != null && made == null && failed == null) {
            percent = montage.percent() / 100f
            delay(300)
        }
    }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("video/mp4")) { uri ->
        val file = made
        if (uri != null && file != null) {
            model.viewModelScopeLaunch {
                withContext(Dispatchers.IO) {
                    context.contentResolver.openOutputStream(uri, "wt")?.use { out -> file.inputStream().use { it.copyTo(out) } }
                }
                Toast.makeText(context, R.string.video_saved, Toast.LENGTH_SHORT).show()
            }
        }
    }
    AlertDialog(
        onDismissRequest = {},
        title = { Text(stringResource(R.string.share_video)) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
                val summed = plan
                when {
                    failed != null -> Text(stringResource(R.string.video_not_made) + (failed?.takeIf { it.isNotEmpty() }?.let { ": $it" } ?: ""))
                    made != null -> Text(stringResource(R.string.video_made))
                    summed != null && !asked -> MontageSummary(summed)
                    summed == null || montage == null -> LinearProgressIndicator(Modifier.fillMaxWidth())
                    else -> {
                        Text(stringResource(R.string.making_video, (percent * 100).toInt()))
                        LinearProgressIndicator(progress = { percent }, Modifier.fillMaxWidth())
                    }
                }
            }
        },
        confirmButton = {
            val summed = plan
            when {
                made != null -> made?.let { file ->
                    Row {
                        TextButton(onClick = { save.launch(file.name) }) { Text(stringResource(R.string.save_to_file)) }
                        TextButton(onClick = { context.shareVideo(file) }) { Text(stringResource(R.string.share)) }
                    }
                }
                summed != null && !asked -> TextButton(onClick = { asked = true }, enabled = summed.actions > 0) {
                    Text(stringResource(R.string.make_video))
                }
            }
        },
        dismissButton = {
            TextButton(onClick = onDismiss) {
                Text(stringResource(if (made == null && failed == null) android.R.string.cancel else R.string.close))
            }
        },
    )
}

/** What the video of the actions of the team holds: how many actions of
 * how many matches, the ones left out and why. */
@Composable
private fun MontageSummary(plan: MontagePlan) = Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
    Text(stringResource(R.string.clips_in_matches, plan.actions, plan.matches.size), fontWeight = FontWeight.Bold)
    if (plan.leftOut.isNotEmpty()) {
        val said = plan.leftOut.map { (opponent, why) ->
            val reason = when (why) {
                LeftOut.NO_VIDEO -> R.string.left_out_no_video
                LeftOut.UNREADABLE -> R.string.left_out_unreadable
                LeftOut.NOT_LINED_UP -> R.string.left_out_not_lined_up
            }
            "$opponent (${stringResource(reason)})"
        }
        Text(
            stringResource(R.string.montage_left_out, said.joinToString(", ")),
            style = MaterialTheme.typography.bodyMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

private fun android.content.Context.shareVideo(file: File) {
    val uri: Uri = FileProvider.getUriForFile(this, "$packageName.files", file)
    val send = Intent(Intent.ACTION_SEND).apply {
        type = "video/mp4"
        putExtra(Intent.EXTRA_STREAM, uri)
        clipData = ClipData.newRawUri(file.name, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    startActivity(Intent.createChooser(send, null))
}
