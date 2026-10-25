package io.github.naighes.scout4all

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Box
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.core.content.FileProvider
import io.github.naighes.scout4all.core.ExportedArchive
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Scout4All
import java.io.File
import java.io.IOException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/** The archives are the zip of the CLI. */
const val ZIP = "application/zip"

/** Shares the archive `export` makes (Quick Share, WhatsApp, e-mail…) or
 * saves it to a file the user chooses (Download, Drive, a USB stick), from a
 * button of the top bar. The archive is made again for each, so it has the
 * latest data. */
@Composable
fun ExportButton(description: String, export: (Scout4All) -> ExportedArchive) = Box {
    val context = LocalContext.current
    val app = context.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    var open by remember { mutableStateOf(false) }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(ZIP)) { uri ->
        uri?.let {
            scope.launch {
                context.report {
                    val archive = app.core(export)
                    context.write(it, archive.bytes)
                    R.string.archive_saved
                }
            }
        }
    }
    IconButton(onClick = { open = true }) {
        Icon(Icons.Filled.Share, contentDescription = description)
    }
    DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
        DropdownMenuItem(
            text = { Text(stringResource(R.string.share)) },
            onClick = {
                open = false
                scope.launch {
                    context.report {
                        context.share(app.core(export))
                        null
                    }
                }
            },
        )
        DropdownMenuItem(
            text = { Text(stringResource(R.string.save_to_file)) },
            onClick = {
                open = false
                scope.launch {
                    context.report {
                        save.launch(app.core(export).fileName)
                        null
                    }
                }
            },
        )
    }
}

/** Runs `action`, then shows the message it returns, or what went wrong. */
private suspend fun Context.report(action: suspend () -> Int?) {
    val message = try {
        action()?.let(::getString)
    } catch (e: MobileException) {
        e.reason
    } catch (e: IOException) {
        e.localizedMessage ?: getString(R.string.archive_not_saved)
    }
    message?.let { Toast.makeText(this, it, Toast.LENGTH_LONG).show() }
}

/** The share sheet, with the archive in a file of the cache that the
 * receiving app may read. */
private suspend fun Context.share(archive: ExportedArchive) {
    val file = withContext(Dispatchers.IO) {
        File(cacheDir, EXPORTS).apply { mkdirs() }.resolve(archive.fileName).apply { writeBytes(archive.bytes) }
    }
    val uri = FileProvider.getUriForFile(this, "$packageName.files", file)
    val send = Intent(Intent.ACTION_SEND).apply {
        type = ZIP
        putExtra(Intent.EXTRA_STREAM, uri)
        // the preview of the sheet shows the name of the file
        clipData = ClipData.newRawUri(archive.fileName, uri)
        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
    }
    startActivity(Intent.createChooser(send, null))
}

private suspend fun Context.write(uri: Uri, bytes: ByteArray) = withContext(Dispatchers.IO) {
    contentResolver.openOutputStream(uri, "wt")?.use { it.write(bytes) }
        ?: throw IOException(getString(R.string.archive_not_saved))
}

/** The folder of the cache with the archives shared, as in `file_paths.xml`. */
private const val EXPORTS = "exports"
