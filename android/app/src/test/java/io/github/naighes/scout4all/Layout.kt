package io.github.naighes.scout4all

import android.graphics.Bitmap
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.asAndroidBitmap
import androidx.compose.ui.semantics.SemanticsActions
import androidx.compose.ui.semantics.SemanticsNode
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.semantics.getOrNull
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.captureToImage
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onRoot
import androidx.compose.ui.text.TextLayoutResult
import androidx.compose.ui.unit.dp
import java.io.File
import kotlin.math.abs
import org.junit.rules.TestRule
import org.junit.runner.Description
import org.junit.runners.model.Statement
import org.robolectric.RuntimeEnvironment

/** A screen the app must fit, as `layout_tests.rs` for the TUI: the window
 * without the system bars, which Robolectric doesn't have (24dp of status
 * bar, 48dp of navigation with three buttons). `scrolls`: only not broken,
 * scrolling is fine. */
data class Size(val name: String, val qualifiers: String, val fontScale: Float = 1f, val scrolls: Boolean = false) {
    override fun toString() = name
}

val sizes = listOf(
    Size("320x568", "w320dp-h496dp-port", scrolls = true),
    // 16:9, as the phones of Android 9
    Size("360x640", "w360dp-h568dp-port", scrolls = true),
    // 20:9, the phones of today
    Size("360x800", "w360dp-h728dp-port"),
    // the phone of the user
    Size("392x872", "w392dp-h800dp-port"),
    Size("landscape", "w824dp-h368dp-land"),
    Size("tablet", "w800dp-h1208dp-port"),
    Size("font 1.3", "w392dp-h800dp-port", fontScale = 1.3f),
    Size("font 1.5", "w392dp-h800dp-port", fontScale = 1.5f),
    Size("font 2", "w392dp-h800dp-port", fontScale = 2f),
)

/** Sets the size before the activity of the compose rule starts: it goes
 * first (`order = 0`). */
class SizeRule(private val size: Size) : TestRule {
    override fun apply(base: Statement, description: Description) = object : Statement() {
        override fun evaluate() {
            RuntimeEnvironment.setQualifiers(size.qualifiers)
            RuntimeEnvironment.setFontScale(size.fontScale)
            base.evaluate()
        }
    }
}

/** The screen as a PNG in the folder of `-Pshots=<folder>`, to look at
 * it; nothing without. */
fun ComposeContentTestRule.shot(name: String) {
    val folder = System.getProperty("shots")?.takeIf { it.isNotEmpty() } ?: return
    waitForIdle()
    val image = onRoot().captureToImage().asAndroidBitmap()
    File(folder, "${name.replace(' ', '-')}.png").outputStream().use { image.compress(Bitmap.CompressFormat.PNG, 100, it) }
}

/** What doesn't fit on the screen shown: something to scroll, a text cut
 * by its lines or by a parent (or out of the window); where it `scrolls`,
 * only a text cut by its lines. */
fun ComposeContentTestRule.misfits(scrolls: Boolean): List<String> {
    waitForIdle()
    val nodes = onAllNodes(SemanticsMatcher("any") { true }, useUnmergedTree = true).fetchSemanticsNodes()
    return nodes.flatMap { node ->
        listOfNotNull(
            node.config.getOrNull(SemanticsProperties.VerticalScrollAxisRange)
                ?.takeIf { !scrolls && it.maxValue() > 0f }
                ?.let { "scrolls by ${it.maxValue()}px: ${node.describe()}" },
            node.textLayout()?.takeIf { it.cut() }?.let { "text cut: ${node.describe()}" },
            node.takeIf { !scrolls && it.text() != null && it.clipped() }?.let { "text clipped: ${node.describe()}" },
        )
    }
}

/** What TalkBack or a finger can't use: something to touch without a
 * label to read, or two neighbours closer than 48dp. Compose grows the
 * touch area of whatever is smaller up to 48dp (as Material does for its
 * icon buttons and chips), but between two neighbours the touch goes to the
 * nearer, or to the one on top: each gets a square of 48dp around its
 * middle only if their middles are 48dp apart. Neighbours are in the same
 * parent: a button floating over a list is above it, not beside. The tree
 * is the merged one, as TalkBack reads it. */
fun ComposeContentTestRule.inaccessible(): List<String> {
    val unlabeled = onAllNodes(hasClickAction()).fetchSemanticsNodes()
        .filter { it.text() == null && it.description() == null }
        .map { "no label: ${it.describe()}" }
    return unlabeled + hidden() + crowded().map { (a, b) -> "closer than 48dp: ${a.describe()}, ${b.describe()}" }
}

/** Texts TalkBack doesn't read: with a description, it reads that instead
 * of the texts. A description of something inside what's read as one (an
 * icon in a row) hides the texts of the row: the row is read as the icon. */
private fun ComposeContentTestRule.hidden(): List<String> {
    val merged = onAllNodes(SemanticsMatcher("any") { true }).fetchSemanticsNodes().associateBy { it.id }
    return onAllNodes(SemanticsMatcher("merges") { it.config.isMergingSemanticsOfDescendants }, useUnmergedTree = true)
        .fetchSemanticsNodes()
        .mapNotNull { node -> merged[node.id]?.takeIf { it.text() != null }?.let { node to it } }
        .flatMap { (node, read) ->
            node.merged().mapNotNull { it.description() }.map { "read as \"$it\": ${read.text()}" }
        }
}

/** What is read as part of this, all down: not what is read on its own. */
private fun SemanticsNode.merged(): List<SemanticsNode> =
    children.filterNot { it.config.isMergingSemanticsOfDescendants }.flatMap { listOf(it) + it.merged() }

/** The neighbours closer than 48dp, as `inaccessible` tells. */
fun ComposeContentTestRule.crowded(): List<Pair<SemanticsNode, SemanticsNode>> {
    waitForIdle()
    val nodes = onAllNodes(hasClickAction()).fetchSemanticsNodes()
    return nodes.flatMapIndexed { i, a ->
        nodes.drop(i + 1).filter { b -> a.parent?.id == b.parent?.id && a.crowds(b) }.map { b -> a to b }
    }
}

/** Their middles are closer than 48dp, both across and down; a border of
 * 1dp they may share. */
private fun SemanticsNode.crowds(other: SemanticsNode): Boolean {
    val apart = bounds().center - other.bounds().center
    val side = with(layoutInfo.density) { 47.dp.toPx() }
    return abs(apart.x) < side && abs(apart.y) < side
}

fun SemanticsNode.bounds() = Rect(positionInRoot, size.toSize())

private fun SemanticsNode.description(): String? =
    config.getOrNull(SemanticsProperties.ContentDescription)?.joinToString(" ")?.takeIf { it.isNotBlank() }

private fun SemanticsNode.textLayout(): TextLayoutResult? {
    val results = mutableListOf<TextLayoutResult>()
    return config.getOrNull(SemanticsActions.GetTextLayoutResult)?.action?.invoke(results)
        ?.takeIf { it }?.let { results.firstOrNull() }
}

/** Lines left out, an ellipsis, or a line wider than the text: not
 * `hasVisualOverflow`, which here says a text narrower than its constraints
 * overflows. A line is never wider than the whole text on one line: laid
 * out without a maximum width (a tab of a scrollable row), Android 9 puts
 * its right edge further. */
private fun TextLayoutResult.cut(): Boolean =
    didOverflowHeight || multiParagraph.didExceedMaxLines ||
        (0 until lineCount).any {
            isLineEllipsized(it) ||
                minOf(getLineRight(it) - getLineLeft(it), multiParagraph.maxIntrinsicWidth) > size.width + 0.5f
        }

private fun SemanticsNode.text(): String? =
    config.getOrNull(SemanticsProperties.Text)?.joinToString(" ")?.takeIf { it.isNotBlank() }

/** The node with `text` is shown whole: not cut by a parent nor by the
 * window. */
fun ComposeContentTestRule.assertWhole(matcher: SemanticsMatcher) {
    val node = onNode(matcher).fetchSemanticsNode()
    check(!node.clipped()) { "not whole: ${node.describe()}" }
}

/** Smaller than itself: a parent or the window cut it. */
private fun SemanticsNode.clipped(): Boolean {
    val whole = bounds()
    val shown = boundsInRoot
    return shown.width < whole.width - 1f || shown.height < whole.height - 1f
}

private fun androidx.compose.ui.unit.IntSize.toSize() =
    androidx.compose.ui.geometry.Size(width.toFloat(), height.toFloat())

fun SemanticsNode.describe(): String =
    text() ?: description() ?: "node $id"
