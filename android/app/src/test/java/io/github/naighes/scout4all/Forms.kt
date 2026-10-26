package io.github.naighes.scout4all

import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.SemanticsMatcher
import androidx.compose.ui.test.SemanticsNodeInteraction
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextReplacement

/** The text field with `label`. */
fun ComposeContentTestRule.field(label: String): SemanticsNodeInteraction =
    onNode(hasSetTextAction() and hasText(label))

fun ComposeContentTestRule.type(label: String, text: String) {
    field(label).performScrollTo().performTextReplacement(text)
}

fun ComposeContentTestRule.save() {
    onNode(hasText(string(R.string.save)) and androidx.compose.ui.test.hasClickAction()).performClick()
    waitForIdle()
}

private val hasError = SemanticsMatcher.keyIsDefined(SemanticsProperties.Error)

/** The field shows the error of the core, and nothing else does. */
fun ComposeContentTestRule.assertInvalid(label: String) {
    waitUntil(5_000) { onAllNodes(hasError).fetchSemanticsNodes().isNotEmpty() }
    field(label).assert(hasError)
}

fun ComposeContentTestRule.assertValid(label: String) {
    field(label).assert(!hasError)
}
