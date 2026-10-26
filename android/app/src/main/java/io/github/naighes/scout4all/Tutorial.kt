package io.github.naighes.scout4all

import androidx.activity.compose.BackHandler
import androidx.annotation.StringRes
import androidx.compose.animation.core.LinearEasing
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.WindowInsets
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawing
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.widthIn
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.BlendMode
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.CompositingStrategy
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.PointerEvent
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerEventType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.layout.LayoutCoordinates
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.layout.positionInRoot
import androidx.compose.ui.node.CompositionLocalConsumerModifierNode
import androidx.compose.ui.node.GlobalPositionAwareModifierNode
import androidx.compose.ui.node.ModifierNodeElement
import androidx.compose.ui.node.ObserverModifierNode
import androidx.compose.ui.node.observeReads
import androidx.compose.ui.node.PointerInputModifierNode
import androidx.compose.ui.node.currentValueOf
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.relocation.bringIntoView
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntRect
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.roundToIntRect
import androidx.compose.ui.window.DialogProperties
import kotlin.math.roundToInt
import kotlinx.coroutines.launch

/** A step of the tutorial: what its caption says, and what ends it. */
sealed interface TutorialStep {
    @get:StringRes val caption: Int

    /** The target, if any: in the hole of the veil. */
    val target: String?

    /** Ends when `target` is touched: the hand taps on it. */
    data class Touch(override val target: String, @StringRes override val caption: Int) : TutorialStep

    /** Ends when `until` is reached, swiping up from `target`: the hand
     * swipes. */
    data class SwipeUp(override val target: String, val until: String, @StringRes override val caption: Int) :
        TutorialStep

    /** Only to read, with `target` shown if any: ends with "Next". */
    data class Read(override val target: String?, @StringRes override val caption: Int) : TutorialStep

    /** The last word, over everything (the statistics too, in a window of
     * their own): ends with "Finish". */
    data class Done(@StringRes override val caption: Int) : TutorialStep {
        override val target: String? get() = null
    }
}

/** What the statistics reach once opened. */
const val STATS_OPENED = "stats-opened"

/** The tutorial of the app, one for the activity: once started, one step
 * after the other, until the last one or until skipped; then the `onEnd`
 * of the start. The screens tell it where their targets are
 * (`Modifier.tutorial`), when one is touched and what is reached, even
 * when it isn't going on. */
class Tutorial {
    private var steps by mutableStateOf<List<TutorialStep>>(emptyList())
    private var onEnd: () -> Unit = {}

    var index by mutableIntStateOf(0)
        private set

    /** The step going on, none when the tutorial isn't. */
    val step: TutorialStep? get() = steps.getOrNull(index)

    val active: Boolean get() = step != null

    val count: Int get() = steps.size

    /** Where the targets shown are, in the window. */
    internal val targets = mutableStateMapOf<String, Rect>()

    fun start(steps: List<TutorialStep>, onEnd: () -> Unit) {
        this.steps = steps
        this.onEnd = onEnd
        index = 0
    }

    internal fun place(target: String, bounds: Rect) {
        targets[target] = bounds
    }

    internal fun remove(target: String) {
        targets.remove(target)
    }

    /** `target` touched: the step that waits for it is over. */
    fun touched(target: String) {
        if (step.let { it is TutorialStep.Touch && it.target == target }) next()
    }

    /** `what` reached (the statistics opened): the step that waits for it
     * is over. */
    fun reached(what: String) {
        if (step.let { it is TutorialStep.SwipeUp && it.until == what }) next()
    }

    fun next() {
        if (!active) return
        index++
        if (index == steps.size) end()
    }

    fun skip() {
        if (active) end()
    }

    private fun end() {
        steps = emptyList()
        index = 0
        onEnd()
    }
}

/** The tutorial of the app, if any (not in the tests of a screen alone):
 * the screens tell it about their targets. */
val LocalTutorial = staticCompositionLocalOf<Tutorial?> { null }

/** A target of the tutorial: where it is, and when it's touched. The touch
 * goes on to what is under, as without the tutorial. */
fun Modifier.tutorial(target: String): Modifier = this then TutorialTarget(target)

private data class TutorialTarget(val target: String) : ModifierNodeElement<TutorialTargetNode>() {
    override fun create() = TutorialTargetNode(target)

    override fun update(node: TutorialTargetNode) {
        if (node.target != target) {
            node.tutorial?.remove(node.target)
            node.target = target
        }
    }
}

private class TutorialTargetNode(var target: String) :
    Modifier.Node(),
    GlobalPositionAwareModifierNode,
    PointerInputModifierNode,
    CompositionLocalConsumerModifierNode,
    ObserverModifierNode {
    var tutorial: Tutorial? = null

    override fun onAttach() {
        tutorial = currentValueOf(LocalTutorial)
        scrollToWhenItsTurn()
    }

    override fun onObservedReadsChanged() = scrollToWhenItsTurn()

    // its step started: in sight, if in something that scrolls
    private fun scrollToWhenItsTurn() = observeReads {
        if (tutorial?.step?.target == target) coroutineScope.launch { bringIntoView() }
    }

    override fun onDetach() {
        tutorial?.remove(target)
        tutorial = null
    }

    override fun onGloballyPositioned(coordinates: LayoutCoordinates) {
        tutorial?.place(target, coordinates.boundsInRoot())
    }

    // before what's under: a click consumes its touch
    override fun onPointerEvent(pointerEvent: PointerEvent, pass: PointerEventPass, bounds: IntSize) {
        val released = pass == PointerEventPass.Initial && pointerEvent.type == PointerEventType.Release
        val inside = pointerEvent.changes.any { c ->
            c.position.x in 0f..bounds.width.toFloat() && c.position.y in 0f..bounds.height.toFloat()
        }
        if (released && inside) tutorial?.touched(target)
    }

    override fun onCancelPointerInput() {}
}

/** The veil over the app, with a hole on the target of the step: only the
 * target can be touched, the hand shows how; the caption says what to do,
 * with "Skip" always there. The system back does nothing meanwhile. */
@Composable
fun TutorialOverlay(tutorial: Tutorial) {
    val step = tutorial.step ?: return
    if (step is TutorialStep.Done) return Finish(step, tutorial)
    BackHandler {}
    var origin by remember { mutableStateOf(Offset.Zero) }
    val margin = with(LocalDensity.current) { HoleMargin.toPx() }
    val finger = with(LocalDensity.current) { Finger.roundToPx() }
    val hole = step.target?.let { tutorial.targets[it] }?.translate(-origin)?.inflate(margin)
    // a step to read: nothing to touch but its buttons
    val open = hole.takeIf { step !is TutorialStep.Read }
    Box(Modifier.fillMaxSize().onGloballyPositioned { origin = it.positionInRoot() }) {
        Veil(hole)
        Blockers(open)
        val swipe = step is TutorialStep.SwipeUp
        if (open != null) Hand(open, swipe)
        // the finger by the hole in sight too
        val pointed = open?.let { o ->
            val at = fingerAt(o, margin, finger, swipe)
            Rect(minOf(o.left, at.x.toFloat()), o.top, maxOf(o.right, at.x + finger.toFloat()), maxOf(o.bottom, at.y + finger.toFloat()))
        } ?: hole
        CaptionBy(pointed, tutorial.targets["score"]?.translate(-origin)) { Caption(step, tutorial) }
    }
}

/** The caption by the hole: above or below it, on the side with more room
 * first, else beside it; the first place where it leaves the hole and
 * `inSight` (the score) uncovered, else the first. At the bottom without a
 * hole. */
@Composable
private fun CaptionBy(hole: Rect?, inSight: Rect?, caption: @Composable () -> Unit) {
    val insets = WindowInsets.safeDrawing
    val narrowest = with(LocalDensity.current) { CaptionBeside.roundToPx() }
    Layout(content = caption, modifier = Modifier.fillMaxSize()) { measurables, constraints ->
        val caption = measurables.first()
        val area = IntRect(
            insets.getLeft(this, layoutDirection),
            insets.getTop(this),
            (constraints.maxWidth - insets.getRight(this, layoutDirection)).coerceAtLeast(0),
            (constraints.maxHeight - insets.getBottom(this)).coerceAtLeast(0),
        )
        val places = captionPlaces(hole?.roundToIntRect(), area, narrowest) { width -> caption.maxIntrinsicHeight(width) }
        val covers = listOfNotNull(hole, inSight).map { it.roundToIntRect() }
        val place = places.firstOrNull { p -> covers.none { it.overlaps(p) } } ?: places.first()
        val placeable = caption.measure(Constraints(maxWidth = place.width, maxHeight = area.height.coerceAtLeast(0)))
        layout(constraints.maxWidth, constraints.maxHeight) {
            placeable.place(place.left + (place.width - placeable.width) / 2, place.top)
        }
    }
}

/** Not beside the hole if narrower. */
private val CaptionBeside = 240.dp

/** Where a caption `height(width)` high can go in `area` by `hole`, in
 * order: above or below it (where there's more room first), then right
 * and left of it, if there's room; at the bottom without a hole. Each in
 * `area`, where it fits. */
internal fun captionPlaces(hole: IntRect?, area: IntRect, narrowest: Int, height: (Int) -> Int): List<IntRect> {
    fun at(left: Int, top: Int, width: Int): IntRect {
        val h = height(width)
        val y = top.coerceIn(area.top, (area.bottom - h).coerceAtLeast(area.top))
        return IntRect(left, y, left + width, y + h)
    }
    val full = height(area.width)
    if (hole == null) return listOf(at(area.left, area.bottom - full, area.width))
    val below = at(area.left, hole.bottom, area.width)
    val above = at(area.left, hole.top - full, area.width)
    val vertical = if (hole.center.y < area.center.y) listOf(below, above) else listOf(above, below)
    val beside = listOf(hole.right to area.right - hole.right, area.left to hole.left - area.left)
        .filter { (_, width) -> width >= narrowest }
        .map { (left, width) -> at(left, hole.center.y - height(width) / 2, width) }
    return vertical + beside
}

/** The last word, in a dialog: over the statistics too. */
@Composable
private fun Finish(step: TutorialStep.Done, tutorial: Tutorial) = AlertDialog(
    onDismissRequest = {},
    properties = DialogProperties(dismissOnBackPress = false, dismissOnClickOutside = false),
    text = { Text(stringResource(step.caption), style = MaterialTheme.typography.bodyLarge) },
    confirmButton = { Button(onClick = tutorial::next) { Text(stringResource(R.string.tutorial_finish)) } },
)

/** Around the target, so that the whole of it shows. */
internal val HoleMargin = 6.dp

@Composable
private fun Veil(hole: Rect?) {
    val radius = with(LocalDensity.current) { 12.dp.toPx() }
    // offscreen: the hole clears the veil only
    Canvas(Modifier.fillMaxSize().graphicsLayer { compositingStrategy = CompositingStrategy.Offscreen }) {
        drawRect(Color.Black.copy(alpha = 0.72f))
        hole?.let { drawRoundRect(Color.Black, it.topLeft, it.size, CornerRadius(radius), blendMode = BlendMode.Clear) }
    }
}

/** What takes the touches around the hole, or everywhere without one: the
 * hole is left to what is under. */
@Composable
private fun Blockers(hole: Rect?) = Layout(
    content = { repeat(4) { Box(Modifier.blockTouches()) } },
    modifier = Modifier.fillMaxSize(),
) { measurables, constraints ->
    val width = constraints.maxWidth
    val height = constraints.maxHeight
    val top = hole?.top?.roundToInt()?.coerceIn(0, height) ?: height
    val bottom = hole?.bottom?.roundToInt()?.coerceIn(top, height) ?: height
    val left = hole?.left?.roundToInt()?.coerceIn(0, width) ?: 0
    val right = hole?.right?.roundToInt()?.coerceIn(left, width) ?: width
    // above, below, left, right of the hole
    val areas = listOf(
        IntOffset(0, 0) to IntSize(width, top),
        IntOffset(0, bottom) to IntSize(width, height - bottom),
        IntOffset(0, top) to IntSize(left, bottom - top),
        IntOffset(right, top) to IntSize(width - right, bottom - top),
    )
    val placeables = measurables.zip(areas) { m, (_, size) -> m.measure(Constraints.fixed(size.width, size.height)) }
    layout(width, height) {
        placeables.zip(areas) { p, (at, _) -> p.place(at) }
    }
}

private fun Modifier.blockTouches() = pointerInput(Unit) {
    awaitPointerEventScope {
        while (true) awaitPointerEvent(PointerEventPass.Initial).changes.forEach { it.consume() }
    }
}

/** A finger and a ring that grows from its tip: tapping, or going up for a
 * swipe. */
@Composable
private fun Hand(hole: Rect, swipe: Boolean) {
    val transition = rememberInfiniteTransition(label = "hand")
    val phase by transition.animateFloat(
        initialValue = 0f,
        targetValue = 1f,
        animationSpec = infiniteRepeatable(tween(1400, easing = LinearEasing), RepeatMode.Restart),
        label = "phase",
    )
    val density = LocalDensity.current
    val rise = with(density) { if (swipe) 96.dp.toPx() * phase else 0f }
    val finger = with(density) { Finger.roundToPx() }
    val margin = with(density) { HoleMargin.toPx() }
    // white on a dark edge: seen on the amber of a button too
    Canvas(Modifier.fillMaxSize()) {
        val radius = 18.dp.toPx() + 22.dp.toPx() * phase
        val center = tipAt(hole, margin, finger, swipe) - Offset(0f, rise)
        val alpha = 1f - phase
        drawCircle(Color.Black.copy(alpha = alpha * 0.6f), radius, center, style = Stroke(6.dp.toPx()))
        drawCircle(Color.White.copy(alpha = alpha), radius, center, style = Stroke(3.dp.toPx()))
    }
    Icon(
        painterResource(R.drawable.ic_touch),
        contentDescription = null,
        tint = Color.White,
        modifier = Modifier.size(Finger).offset { fingerAt(hole, margin, finger, swipe) - IntOffset(0, rise.roundToInt()) },
    )
}

internal val Finger = 40.dp

/** Where the tip of the finger, `finger` big, touches the target in
 * `hole` (`margin` around it), so that the words of the target show: on
 * its bottom edge, in the middle; for a swipe, which goes up, the whole
 * finger on the right of it (from the middle it went up over the words). */
internal fun tipAt(hole: Rect, margin: Float, finger: Int, swipe: Boolean = false) =
    Offset(
        if (swipe) hole.right - margin + finger * 11 / 24 else hole.center.x,
        hole.bottom - margin,
    )

/** Where the finger goes, `finger` big: its tip at `tipAt`. */
internal fun fingerAt(hole: Rect, margin: Float, finger: Int, swipe: Boolean = false): IntOffset {
    val tip = tipAt(hole, margin, finger, swipe)
    return IntOffset(tip.x.roundToInt() - finger * 11 / 24, tip.y.roundToInt() - finger / 8)
}

/** What to do, how far the tutorial is, and how to end it. */
@Composable
private fun Caption(step: TutorialStep, tutorial: Tutorial) = Surface(
    modifier = Modifier.padding(16.dp).widthIn(max = 560.dp).fillMaxWidth(),
    shape = MaterialTheme.shapes.large,
    color = MaterialTheme.colorScheme.surfaceVariant,
    contentColor = MaterialTheme.colorScheme.onSurface,
    tonalElevation = 6.dp,
) {
    Column(Modifier.padding(start = 20.dp, end = 12.dp, top = 16.dp, bottom = 4.dp)) {
        Text(
            stringResource(step.caption),
            style = MaterialTheme.typography.bodyLarge,
            modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
        )
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(
                stringResource(R.string.tutorial_progress, tutorial.index + 1, tutorial.count),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.weight(1f))
            TextButton(onClick = tutorial::skip) { Text(stringResource(R.string.tutorial_skip)) }
            if (step is TutorialStep.Read) Button(onClick = tutorial::next) { Text(stringResource(R.string.tutorial_next)) }
        }
    }
}
