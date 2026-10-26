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
import androidx.compose.foundation.layout.BoxWithConstraints
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
import androidx.compose.foundation.layout.windowInsetsPadding
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
import androidx.compose.ui.node.PointerInputModifierNode
import androidx.compose.ui.node.currentValueOf
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.IntOffset
import androidx.compose.ui.unit.IntSize
import androidx.compose.ui.unit.dp
import kotlin.math.roundToInt

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
}

/** The tutorial going on: one step after the other, until the last one or
 * until skipped; then `onEnd`. The screens tell it where its targets are
 * (`Modifier.tutorial`), when one is touched and what is reached. */
class Tutorial(private val steps: List<TutorialStep>, private val onEnd: () -> Unit) {
    var index by mutableIntStateOf(0)
        private set

    val step: TutorialStep? get() = steps.getOrNull(index)

    val count: Int get() = steps.size

    /** Where the targets shown are, in the window. */
    internal val targets = mutableStateMapOf<String, Rect>()

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
        if (index >= steps.size) return
        index++
        if (index == steps.size) onEnd()
    }

    fun skip() {
        if (index >= steps.size) return
        index = steps.size
        onEnd()
    }
}

/** The tutorial going on, if any: the screens tell it about their targets. */
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
    CompositionLocalConsumerModifierNode {
    var tutorial: Tutorial? = null

    override fun onAttach() {
        tutorial = currentValueOf(LocalTutorial)
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
    BackHandler {}
    var origin by remember { mutableStateOf(Offset.Zero) }
    val margin = with(LocalDensity.current) { HoleMargin.toPx() }
    val hole = step.target?.let { tutorial.targets[it] }?.translate(-origin)?.inflate(margin)
    // a step to read: nothing to touch but its buttons
    val open = hole.takeIf { step !is TutorialStep.Read }
    BoxWithConstraints(Modifier.fillMaxSize().onGloballyPositioned { origin = it.positionInRoot() }) {
        Veil(hole)
        Blockers(open)
        if (open != null) Hand(open, swipe = step is TutorialStep.SwipeUp)
        // on the other half of the screen
        val below = hole == null || hole.center.y < constraints.maxHeight / 2
        Caption(step, tutorial, Modifier.align(if (below) Alignment.BottomCenter else Alignment.TopCenter))
    }
}

/** Around the target, so that the whole of it shows. */
private val HoleMargin = 6.dp

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

/** A ring that grows from the middle of the hole and a finger: tapping, or
 * going up for a swipe. */
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
    val color = MaterialTheme.colorScheme.primary
    Canvas(Modifier.fillMaxSize()) {
        val radius = 18.dp.toPx() + 22.dp.toPx() * phase
        drawCircle(color.copy(alpha = 1f - phase), radius, hole.center - Offset(0f, rise), style = Stroke(3.dp.toPx()))
    }
    val finger = with(density) { 40.dp.roundToPx() }
    Icon(
        painterResource(R.drawable.ic_touch),
        contentDescription = null,
        tint = Color.White,
        modifier = Modifier.size(40.dp).offset {
            // the tip of the finger on the middle of the hole
            IntOffset(hole.center.x.roundToInt() - finger * 11 / 24, (hole.center.y - rise).roundToInt() - finger / 8)
        },
    )
}

/** What to do, how far the tutorial is, and how to end it. */
@Composable
private fun Caption(step: TutorialStep, tutorial: Tutorial, modifier: Modifier) = Surface(
    modifier = modifier.windowInsetsPadding(WindowInsets.safeDrawing).padding(16.dp).widthIn(max = 560.dp).fillMaxWidth(),
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
