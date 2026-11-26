package io.github.naighes.scout4all

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.github.naighes.scout4all.core.Licenses
import io.github.naighes.scout4all.core.ThirdPartyLicense

/** The licenses of the parts made by others the app carries, from the
 * information on the app: what each is, then its text as written. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun LicensesScreen(onBack: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val licenses by produceState<Licenses?>(null) { value = app.core { it.licenses() } }
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(licenses?.title?.replaceFirstChar { it.uppercase() } ?: "", Modifier.title()) },
                navigationIcon = { BackButton(onBack) },
            )
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val l = licenses) {
            null -> Centered(modifier) { CircularProgressIndicator() }
            else -> Column(
                modifier.verticalScroll(rememberScrollState()).padding(16.dp),
                verticalArrangement = Arrangement.spacedBy(16.dp),
            ) {
                Text(l.intro, style = MaterialTheme.typography.bodyLarge)
                l.parts.forEach {
                    HorizontalDivider()
                    Part(it)
                }
            }
        }
    }
}

/** A part: its name, what it's for, its license and where it comes from,
 * then the text of the license (monospaced, as its lines are laid out). */
@Composable
private fun Part(part: ThirdPartyLicense) = Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
    Text(part.name, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
    Text(
        part.purpose.replaceFirstChar { it.uppercase() },
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
    Text(
        "${part.license} · ${part.source}",
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.primary,
    )
    Text(
        part.text,
        Modifier.padding(top = 8.dp),
        style = MaterialTheme.typography.bodySmall,
        fontFamily = FontFamily.Monospace,
    )
}
