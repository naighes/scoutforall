package io.github.naighes.scout4all

import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.testTag
import androidx.compose.material3.Text
import io.github.naighes.scout4all.core.initials
import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.LogoBackdrop
import io.github.naighes.scout4all.core.LogoTone
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.logoBackdrop
import io.github.naighes.scout4all.core.logoTone

/** Tag of the image shown by [LogoBox], for the tests. */
const val SHOWN_IMAGE = "shown-image"

/** Tags of a logo once its tone is read: on a backdrop (it would vanish
 * on the theme) or as it is, for the tests. */
const val LOGO_BACKDROP = "logo-backdrop"
const val LOGO_AS_IT_IS = "logo-as-it-is"

/** The image the bytes hold, null when they are none or no image. */
fun decodeImage(bytes: ByteArray?): ImageBitmap? =
    bytes?.let { BitmapFactory.decodeByteArray(it, 0, it.size) }?.asImageBitmap()

/** The logo of the team in a square: without it `none`, else an empty box,
 * so the names line up. Read once per team. */
@Composable
fun TeamLogo(
    teamId: String,
    size: Dp = 40.dp,
    modifier: Modifier = Modifier,
    none: (@Composable () -> Unit)? = null,
) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val bytes by produceState<ByteArray?>(null, teamId) {
        value = try {
            app.core { it.teamLogo(teamId) }
        } catch (e: MobileException) {
            null
        }
    }
    when {
        bytes == null && none != null -> none()
        else -> TeamLogoBox(bytes, size, modifier)
    }
}

/** The tag of the initials of a `TeamPicture`, for the tests. */
fun initialsTag(letters: String) = "initials-$letters"

/** What stands for a team beside its name, in a square: its logo, or its
 * initials, not read as the name says it. `teamId` is null for a name
 * only. */
@Composable
fun TeamPicture(teamId: String?, name: String, size: Dp = 48.dp, modifier: Modifier = Modifier) {
    val letters: @Composable () -> Unit = { TeamInitials(name, size, modifier) }
    when (teamId) {
        null -> letters()
        else -> TeamLogo(teamId, size, modifier, none = letters)
    }
}

/** The initials of the team in a square, not read as its name says it. */
@Composable
private fun TeamInitials(name: String, size: Dp, modifier: Modifier = Modifier) {
    val shown = initials(name)
    Box(modifier.size(size).clearAndSetSemantics { testTag = initialsTag(shown) }, contentAlignment = Alignment.Center) {
        Text(
            shown,
            style = if (size < 40.dp) MaterialTheme.typography.labelSmall else MaterialTheme.typography.titleMedium,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** `TeamPicture` of a logo already read: `logo` null for a team without
 * one. */
@Composable
fun LogoOrInitials(logo: ByteArray?, name: String, size: Dp, modifier: Modifier = Modifier) {
    when (logo) {
        null -> TeamInitials(name, size, modifier)
        else -> TeamLogoBox(logo, size, modifier)
    }
}

/** The color behind a logo, none when it shows on the theme. */
fun backdropColor(backdrop: LogoBackdrop): Color? = when (backdrop) {
    LogoBackdrop.NONE -> null
    LogoBackdrop.DARK -> LogoBackdropDark
    LogoBackdrop.LIGHT -> LogoBackdropLight
}

/** The logo `bytes` in a square: on the background of the other theme, a
 * bit smaller, when it would vanish on this one (TODO/011 R8.6). Its tone
 * is read once, away from the main thread. */
@Composable
fun TeamLogoBox(bytes: ByteArray?, size: Dp, modifier: Modifier = Modifier) {
    val tone by produceState<LogoTone?>(null, bytes) {
        value = bytes?.let { kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.Default) { logoTone(it) } }
    }
    val dark = MaterialTheme.colorScheme.background.luminance() < 0.5f
    val backdrop = tone?.let { logoBackdrop(it, dark) }
    when (val color = backdrop?.let(::backdropColor)) {
        null -> LogoBox(bytes, size, backdrop?.let { modifier.testTag(LOGO_AS_IT_IS) } ?: modifier)
        else -> Box(
            modifier.size(size).clip(RoundedCornerShape(size * 0.15f)).background(color).testTag(LOGO_BACKDROP),
            contentAlignment = Alignment.Center,
        ) {
            LogoBox(bytes, size * 0.8f)
        }
    }
}

/** The photo of the player (the id of the player is the one of the person)
 * in a square: an empty box without it, so the names line up. */
@Composable
fun PlayerPhoto(playerId: String, size: Dp = 40.dp, modifier: Modifier = Modifier) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val bytes by produceState<ByteArray?>(null, playerId) {
        value = try {
            app.core { it.personPhoto(playerId) }
        } catch (e: MobileException) {
            null
        }
    }
    LogoBox(bytes, size, modifier)
}

/** The photo of the player, decoded once; null without it. */
@Composable
fun photoOf(playerId: String): androidx.compose.ui.graphics.ImageBitmap? {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val bytes by produceState<ByteArray?>(null, playerId) {
        value = try {
            app.core { it.personPhoto(playerId) }
        } catch (e: MobileException) {
            null
        }
    }
    return remember(bytes) { decodeImage(bytes) }
}

/** `bytes` in a square, an empty box when null or not an image. */
@Composable
fun LogoBox(bytes: ByteArray?, size: Dp, modifier: Modifier = Modifier) {
    val image = remember(bytes) { decodeImage(bytes) }
    Box(modifier.size(size)) {
        image?.let {
            Image(it, contentDescription = null, Modifier.size(size).testTag(SHOWN_IMAGE), contentScale = ContentScale.Fit)
        }
    }
}

/** The bytes of the file or picture at `uri`, empty when it can't be read
 * (so not an image either). */
suspend fun readUri(context: android.content.Context, uri: android.net.Uri): ByteArray =
    kotlinx.coroutines.withContext(kotlinx.coroutines.Dispatchers.IO) {
        try {
            context.contentResolver.openInputStream(uri)?.use { it.readBytes() }
        } catch (e: java.io.IOException) {
            null
        } ?: ByteArray(0)
    }
