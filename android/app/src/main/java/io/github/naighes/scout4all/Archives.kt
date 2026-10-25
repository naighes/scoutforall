package io.github.naighes.scout4all

import android.content.ClipData
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Share
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.painterResource
import androidx.compose.ui.res.pluralStringResource
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.core.content.FileProvider
import io.github.naighes.scout4all.core.ArchivePreview
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
    var error by remember { mutableStateOf<String?>(null) }
    error?.let { ErrorDialog(it) { error = null } }
    val save = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument(ZIP)) { uri ->
        uri?.let {
            scope.launch {
                context.report({ error = it }) {
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
                    context.report({ error = it }) {
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
                    context.report({ error = it }) {
                        save.launch(app.core(export).fileName)
                        null
                    }
                }
            },
        )
    }
}

/** An archive chosen to import, with what it holds. */
private class Chosen(val bytes: ByteArray, val fileName: String?, val preview: ArchivePreview)

/** Imports an archive from a file the user chooses (Download, Drive, a USB
 * stick), after showing what it holds: a team, or a match into `teamId`
 * (any team of its players when null). Then shows what was imported. */
@Composable
fun ImportButton(
    description: String,
    teamId: String?,
    onTeam: (String) -> Unit,
    onMatch: (teamId: String, matchId: String) -> Unit,
) {
    val context = LocalContext.current
    val app = context.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    var chosen by remember { mutableStateOf<Chosen?>(null) }
    var error by remember { mutableStateOf<String?>(null) }
    error?.let { ErrorDialog(it) { error = null } }
    // some apps don't say a zip is a zip
    val open = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        uri?.let {
            scope.launch {
                context.report({ error = it }) {
                    val (bytes, fileName) = context.read(it)
                    val preview = app.core { core -> core.previewArchive(bytes, fileName, teamId) }
                    chosen = Chosen(bytes, fileName, preview)
                    null
                }
            }
        }
    }
    IconButton(onClick = { open.launch(arrayOf(ZIP, "application/x-zip-compressed", "application/octet-stream")) }) {
        Icon(painterResource(R.drawable.ic_import), contentDescription = description)
    }
    chosen?.let { c ->
        ImportDialog(c.preview, teamId, onDismiss = { chosen = null }) { into ->
            chosen = null
            scope.launch {
                context.report({ error = it }) {
                    when (into) {
                        null -> {
                            val id = app.core { it.importTeam(c.bytes, c.fileName) }
                            onTeam(id)
                            R.string.team_imported
                        }
                        else -> {
                            val m = app.core { it.importMatch(c.bytes, into) }
                            onMatch(into, m)
                            R.string.match_imported
                        }
                    }
                }
            }
        }
    }
}

/** What the archive holds, to confirm importing it: `onImport` gets the team
 * a match goes into, null for a team. */
@Composable
private fun ImportDialog(
    preview: ArchivePreview,
    teamId: String?,
    onDismiss: () -> Unit,
    onImport: (into: String?) -> Unit,
) {
    val teams = (preview as? ArchivePreview.Match)?.teams.orEmpty()
    var into by remember { mutableStateOf(teamId ?: teams.firstOrNull()?.id) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = {
            Text(stringResource(if (preview is ArchivePreview.Team) R.string.import_team_title else R.string.import_match_title))
        },
        text = {
            Column {
                when (preview) {
                    is ArchivePreview.Team -> {
                        val players = preview.players.toInt()
                        Text(preview.name, style = MaterialTheme.typography.titleMedium)
                        Text("${preview.year} · ${pluralStringResource(R.plurals.players, players, players)}")
                    }
                    is ArchivePreview.Match -> {
                        val place = stringResource(if (preview.home) R.string.home else R.string.away)
                        Text(preview.opponent, style = MaterialTheme.typography.titleMedium)
                        Text("${matchDate(preview.date)} · $place · ${preview.won}-${preview.lost}")
                        // which team, when it's opened outside of one
                        if (teamId == null) TeamChoices(teams, into) { into = it }
                    }
                }
            }
        },
        confirmButton = {
            TextButton(onClick = { onImport(if (preview is ArchivePreview.Team) null else into) }) {
                Text(stringResource(R.string.import_archive))
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text(stringResource(R.string.cancel)) } },
    )
}

/** The team of the match: its name when there's one, to choose otherwise. */
@Composable
private fun TeamChoices(teams: List<io.github.naighes.scout4all.core.TeamChoice>, chosen: String?, onChoose: (String) -> Unit) {
    Text(
        stringResource(R.string.into_team),
        modifier = Modifier.padding(top = 16.dp),
        style = MaterialTheme.typography.labelLarge,
    )
    when (teams.size) {
        1 -> Text(teams[0].name)
        else -> teams.forEach { team ->
            Row(
                Modifier.fillMaxWidth().selectable(selected = team.id == chosen, onClick = { onChoose(team.id) }),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                RadioButton(selected = team.id == chosen, onClick = null)
                Text(team.name, modifier = Modifier.padding(start = 8.dp))
            }
        }
    }
}

/** The content of the file and its name, when the app that has it says. */
private suspend fun Context.read(uri: Uri): Pair<ByteArray, String?> = withContext(Dispatchers.IO) {
    val bytes = contentResolver.openInputStream(uri)?.use { it.readBytes() }
        ?: throw IOException(getString(R.string.file_not_read))
    val name = contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use {
        if (it.moveToFirst()) it.getString(0) else null
    }
    bytes to name
}

/** Runs `action`, then shows the message it returns, briefly; what went
 * wrong goes to `onError`, to be read in full. */
private suspend fun Context.report(onError: (String) -> Unit, action: suspend () -> Int?) {
    try {
        action()?.let { Toast.makeText(this, it, Toast.LENGTH_SHORT).show() }
    } catch (e: MobileException) {
        onError(e.reason)
    } catch (e: IOException) {
        onError(e.localizedMessage ?: getString(R.string.file_not_read))
    }
}

/** Why an archive wasn't exported or imported. */
@Composable
private fun ErrorDialog(message: String, onDismiss: () -> Unit) = AlertDialog(
    onDismissRequest = onDismiss,
    text = { Text(message) },
    confirmButton = { TextButton(onClick = onDismiss) { Text(stringResource(android.R.string.ok)) } },
)

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
