package io.github.naighes.scout4all

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.provider.Settings
import android.widget.Toast
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.ListItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import io.github.naighes.scout4all.core.version

private const val REPOSITORY = "https://github.com/naighes/scoutforall"
// the privacy policy, also the one on Google Play: the text of DATA-PRIVACY
private const val PRIVACY = "https://naighes.github.io/scoutforall/privacy/"
// the e-mail address that opens an issue on Linear, the tracker of the project
private const val REPORTS = "scout4all-0fd59357fbe3@intake.linear.app"

/** The settings: the language of the app (the system chooses it), the
 * statistics sent, the terms of use, the information on the app. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(onBack: () -> Unit, onTerms: () -> Unit, onAbout: () -> Unit, onTutorial: () -> Unit = {}) {
    val context = LocalContext.current
    val locale = LocalConfiguration.current.locales[0]
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.settings), Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState())) {
            // a language for the app only since Android 13: before, the one
            // of the system
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
                Row(
                    stringResource(R.string.language),
                    locale.getDisplayLanguage(locale).replaceFirstChar { it.titlecase(locale) },
                ) {
                    context.open(
                        Intent(
                            Settings.ACTION_APP_LOCALE_SETTINGS,
                            Uri.fromParts("package", context.packageName, null),
                        ),
                    )
                }
            }
            StatisticsSwitch()
            Row(stringResource(R.string.tutorial), stringResource(R.string.tutorial_detail), onTutorial)
            Row(stringResource(R.string.terms_of_use), null, onTerms)
            Row(stringResource(R.string.about), null, onAbout)
        }
    }
}

/** The version, the license, the data, the code, and how to report a
 * problem: an e-mail that opens an issue on Linear, with the version and
 * the device. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AboutScreen(onBack: () -> Unit) {
    val context = LocalContext.current
    val version = version()
    val reportTitle = stringResource(R.string.report_title)
    val reportBody = stringResource(R.string.report_body, device())
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(stringResource(R.string.about), Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
            )
        },
    ) { padding ->
        Column(Modifier.fillMaxSize().padding(padding).verticalScroll(rememberScrollState())) {
            Row(stringResource(R.string.version), version, null)
            Row(stringResource(R.string.license), stringResource(R.string.license_name)) {
                context.browse("$REPOSITORY/blob/master/LICENSE")
            }
            Row(stringResource(R.string.data_privacy), null) {
                context.browse(PRIVACY)
            }
            Row(stringResource(R.string.source_code), REPOSITORY.removePrefix("https://")) {
                context.browse(REPOSITORY)
            }
            HorizontalDivider()
            Row(stringResource(R.string.report_problem), stringResource(R.string.report_problem_detail)) {
                context.report(reportTitle, reportBody)
            }
        }
    }
}

/** A row of the settings, touched when there's `onClick`. */
@Composable
private fun Row(headline: String, supporting: String?, onClick: (() -> Unit)?) = ListItem(
    modifier = onClick?.let { Modifier.clickable(onClick = it) } ?: Modifier,
    headlineContent = { Text(headline) },
    supportingContent = supporting?.let { { Text(it) } },
)

/** What helps to reproduce a problem, at the end of a report. */
fun device(): String = "scout4all ${version()} · Android ${Build.VERSION.RELEASE} · " +
    "${Build.MANUFACTURER} ${Build.MODEL}"

/** An e-mail ready to send, that opens an issue on Linear. */
fun Context.report(title: String, body: String) = open(
    Intent(Intent.ACTION_SENDTO, Uri.parse("mailto:")).apply {
        putExtra(Intent.EXTRA_EMAIL, arrayOf(REPORTS))
        putExtra(Intent.EXTRA_SUBJECT, "scout4all ${version()}: $title")
        putExtra(Intent.EXTRA_TEXT, body)
    },
)

private fun Context.browse(url: String) = open(Intent(Intent.ACTION_VIEW, Uri.parse(url)))

/** Another app opens it, if there's one. */
private fun Context.open(intent: Intent) = try {
    startActivity(intent)
} catch (_: ActivityNotFoundException) {
    Toast.makeText(this, R.string.no_app_to_open, Toast.LENGTH_SHORT).show()
}
