package io.github.naighes.scout4all

import android.content.Context
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.toggleable
import androidx.compose.material3.ListItem
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.unit.dp
import androidx.work.BackoffPolicy
import androidx.work.Constraints
import androidx.work.CoroutineWorker
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.WorkerParameters
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Uploads
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.launch

/** Sends the matches of the upload queue: the core says whether some are
 * left, to try again later. */
class UploadWorker(context: Context, params: WorkerParameters) : CoroutineWorker(context, params) {
    override suspend fun doWork(): Result = try {
        when ((applicationContext as Scout4AllApplication).core { it.uploadPending() }) {
            Uploads.DONE -> Result.success()
            Uploads.RETRY -> Result.retry()
        }
    } catch (_: CorePanic) {
        // the queue stays: sent again by the app started again
        Result.failure()
    }
}

/** Sends the upload queue as soon as there's a network, in the background:
 * also once the app is closed or the phone restarted. With `KEEP` nothing
 * changes while a sending is waiting or running; a match just finished
 * goes after it (`APPEND_OR_REPLACE`), in case it just found the queue
 * empty. */
fun Context.sendUploads(policy: ExistingWorkPolicy = ExistingWorkPolicy.KEEP) {
    val request = OneTimeWorkRequestBuilder<UploadWorker>()
        .setConstraints(Constraints(requiredNetworkType = NetworkType.CONNECTED))
        .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 1, TimeUnit.MINUTES)
        .build()
    WorkManager.getInstance(this).enqueueUniqueWork(UPLOADS, policy, request)
}

private const val UPLOADS = "uploads"

/** Whether the statistics of the matches are sent: under the terms and in
 * the settings, saved at once. */
@Composable
fun StatisticsSwitch(modifier: Modifier = Modifier) {
    val context = LocalContext.current
    val app = context.applicationContext as Scout4AllApplication
    val scope = rememberCoroutineScope()
    // null while reading
    var enabled by remember { mutableStateOf<Boolean?>(null) }
    LaunchedEffect(Unit) { enabled = app.core { it.analyticsEnabled() } }
    val checked = enabled ?: return
    val change = { on: Boolean ->
        scope.launch {
            try {
                app.core { it.setAnalyticsEnabled(on) }
                enabled = on
                if (on) context.sendUploads()
            } catch (_: MobileException) {
                // not saved: it stays as it was
            }
        }
        Unit
    }
    ListItem(
        modifier = modifier.toggleable(value = checked, role = Role.Switch, onValueChange = change),
        headlineContent = { Text(stringResource(R.string.send_statistics)) },
        supportingContent = { Text(stringResource(R.string.send_statistics_detail)) },
        trailingContent = { Switch(checked = checked, onCheckedChange = null, modifier = Modifier.padding(start = 8.dp)) },
    )
}
