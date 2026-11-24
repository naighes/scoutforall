package io.github.naighes.scout4all

import android.graphics.BitmapFactory
import androidx.compose.foundation.Image
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.MobileException

/** The image the bytes hold, null when they are none or no image. */
fun decodeImage(bytes: ByteArray?): ImageBitmap? =
    bytes?.let { BitmapFactory.decodeByteArray(it, 0, it.size) }?.asImageBitmap()

/** The logo of the team in a square: an empty box when it has none, so the
 * names line up. Read once per team. */
@Composable
fun TeamLogo(teamId: String, size: Dp = 40.dp, modifier: Modifier = Modifier) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val bytes by produceState<ByteArray?>(null, teamId) {
        value = try {
            app.core { it.teamLogo(teamId) }
        } catch (e: MobileException) {
            null
        }
    }
    LogoBox(bytes, size, modifier)
}

/** `bytes` in a square, an empty box when null or not an image. */
@Composable
fun LogoBox(bytes: ByteArray?, size: Dp, modifier: Modifier = Modifier) {
    val image = remember(bytes) { decodeImage(bytes) }
    Box(modifier.size(size)) {
        image?.let {
            Image(it, contentDescription = null, Modifier.size(size), contentScale = ContentScale.Fit)
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
