package io.github.naighes.scout4all

import androidx.annotation.DrawableRes
import androidx.annotation.StringRes
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.MobileException

/** What the app tells the user, one look everywhere and as on the
 * desktop (`notice.rs`): a note under the field to correct, a dialog for
 * what went wrong doing something, a banner that doesn't stop the
 * scouting. Each level has its color and its icon. */
enum class Level(
    val color: Color,
    @param:DrawableRes val icon: Int,
    @param:StringRes val title: Int,
) {
    /** it didn't go: to correct, or to try again */
    ERROR(Lost, R.drawable.ic_notice_error, R.string.notice_error),

    /** to know before going on */
    WARNING(BreakPoint, R.drawable.ic_notice_warning, R.string.notice_warning),

    /** only told: what went well */
    INFO(SideOut, R.drawable.ic_notice_info, R.string.notice_info),
}

/** Something to tell in a dialog: its level and the text. */
data class Notice(val level: Level, val text: String) {
    companion object {
        fun error(text: String) = Notice(Level.ERROR, text)
        fun warning(text: String) = Notice(Level.WARNING, text)
        fun info(text: String) = Notice(Level.INFO, text)
    }
}

/** The color of the level, light, behind its text (the text staying the
 * theme's): as on the desktop's dark theme. */
private fun Level.tint() = color.copy(alpha = 0.16f)

private fun String.capitalized() = replaceFirstChar { it.uppercase() }

/** The text with the icon of its level, on the level's tint. */
@Composable
private fun Callout(level: Level, text: String, style: TextStyle, modifier: Modifier) = Row(
    modifier
        .background(level.tint(), RoundedCornerShape(8.dp))
        .border(1.dp, level.color.copy(alpha = 0.45f), RoundedCornerShape(8.dp))
        .padding(horizontal = 10.dp, vertical = 4.dp),
    horizontalArrangement = Arrangement.spacedBy(8.dp),
    verticalAlignment = Alignment.CenterVertically,
) {
    Icon(
        painterResource(level.icon),
        contentDescription = null,
        tint = level.color,
        modifier = Modifier.size(with(androidx.compose.ui.platform.LocalDensity.current) { (style.fontSize * 1.3f).toDp() }),
    )
    Text(
        text.capitalized(),
        Modifier.padding(end = 2.dp),
        style = style,
        color = MaterialTheme.colorScheme.onSurface,
    )
}

/** Under a field: what to correct in it, small (the supporting text of a
 * text field, or alone under a field of other kind). */
@Composable
fun FieldNote(text: String, modifier: Modifier = Modifier) = Callout(
    Level.ERROR,
    text,
    MaterialTheme.typography.bodySmall,
    modifier.testTag(FIELD_NOTE_TAG),
)

/** The note of a field, for the supporting text of a text field: none
 * when there's nothing to correct. */
fun noteOf(text: String?): (@Composable () -> Unit)? = text?.let { { FieldNote(it) } }

/** Over the top of a screen that must not stop (the scouting): what
 * happened, across it, until the next action. */
@Composable
fun Banner(level: Level, text: String, modifier: Modifier = Modifier) = Callout(
    level,
    text,
    MaterialTheme.typography.bodyMedium,
    modifier.fillMaxWidth().testTag(BANNER_TAG),
)

/** In place of a page that couldn't be loaded: why, as the banner of the
 * pages that stay, at its top. */
@Composable
fun LoadFailed(message: String, modifier: Modifier = Modifier) = Box(modifier) {
    Banner(Level.ERROR, message, Modifier.padding(16.dp))
}

/** The notice in the middle: the icon of its level in a disc of its tint,
 * the title, the text, OK. Back closes it too. */
@Composable
fun NoticeDialog(notice: Notice, onDismiss: () -> Unit) = AlertDialog(
    onDismissRequest = onDismiss,
    modifier = Modifier.testTag(NOTICE_TAG),
    icon = {
        Box(
            Modifier.size(48.dp).background(notice.level.tint(), CircleShape),
            contentAlignment = Alignment.Center,
        ) {
            Icon(
                painterResource(notice.level.icon),
                contentDescription = null,
                tint = notice.level.color,
                modifier = Modifier.size(28.dp),
            )
        }
    },
    title = { Text(stringResource(notice.level.title)) },
    text = { Text(notice.text.capitalized()) },
    confirmButton = { Button(onClick = onDismiss) { Text(stringResource(R.string.ok)) } },
)

/** What went wrong doing something and no field shows, in a dialog until
 * closed: nothing while there's nothing to tell. */
@Composable
fun FailureDialog(failure: String?, onDismiss: () -> Unit) {
    failure?.let { NoticeDialog(Notice.error(it), onDismiss) }
}

/** What went wrong, told in a dialog: calling what is returned opens it. */
@Composable
fun failureDialog(): (String) -> Unit {
    var failure by rememberSaveable { mutableStateOf<String?>(null) }
    failure?.let { NoticeDialog(Notice.error(it)) { failure = null } }
    return { failure = it }
}

/** A setting not saved, told in a dialog; the setting stays as it was. */
@Composable
fun notSavedDialog(): (MobileException) -> Unit {
    val tell = failureDialog()
    val resources = LocalContext.current.resources
    return { tell(resources.getString(R.string.settings_not_saved, it.reason)) }
}

const val FIELD_NOTE_TAG = "field-note"
const val BANNER_TAG = "banner"
const val NOTICE_TAG = "notice"
