package io.github.naighes.scout4all

import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.semantics.text
import androidx.compose.ui.text.AnnotatedString

// What TalkBack reads. A description hides the texts read with it (an icon
// in a row makes the row read as the icon): inside what is read as one, a
// symbol or an icon is read as a text in its place instead.

/** Read as `text` in its place: a symbol as its word, an icon as a text. */
fun Modifier.readAs(text: String) = clearAndSetSemantics { this.text = AnnotatedString(text) }

/** Not read: what's next to it already says it. */
fun Modifier.notRead() = clearAndSetSemantics {}

/** A title: TalkBack moves from one to the next. */
fun Modifier.title() = semantics { heading() }
