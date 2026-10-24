package io.github.naighes.scout4all

import android.app.Application
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Terms
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.launch

/** Whether the user has seen the current terms of use, null while reading:
 * the activity holds it, the splash waits for it. */
class TermsViewModel(application: Application) : AndroidViewModel(application) {
    private val mutableSeen = MutableStateFlow<Boolean?>(null)
    val seen: StateFlow<Boolean?> = mutableSeen

    init {
        viewModelScope.launch {
            mutableSeen.value = getApplication<Scout4AllApplication>().core { it.termsSeen() }
        }
    }

    /** Saves them as seen: if they can't be saved the app goes on all the
     * same, and shows them again at the next start (as the TUI). */
    suspend fun accept() {
        try {
            getApplication<Scout4AllApplication>().core { it.acceptTerms() }
        } catch (_: MobileException) {
        }
        mutableSeen.value = true
    }
}

/** The terms of use: at the first start (and when they change) with a
 * button to go on, from the settings with the arrow back. */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun TermsScreen(onAccept: (suspend () -> Unit)?, onBack: () -> Unit) {
    val app = LocalContext.current.applicationContext as Scout4AllApplication
    val terms by produceState<Terms?>(null) { value = app.core { it.terms() } }
    val scope = rememberCoroutineScope()
    Scaffold(
        topBar = {
            TopAppBar(
                title = { Text(terms?.title?.replaceFirstChar { it.uppercase() } ?: "") },
                navigationIcon = { if (onAccept == null) BackButton(onBack) },
            )
        },
        bottomBar = {
            if (onAccept != null) {
                Button(
                    onClick = { scope.launch { onAccept() } },
                    enabled = terms != null,
                    modifier = Modifier
                        .fillMaxWidth()
                        .navigationBarsPadding()
                        .padding(16.dp)
                        .heightIn(min = 48.dp),
                ) {
                    Text(stringResource(R.string.accept_terms))
                }
            }
        },
    ) { padding ->
        val modifier = Modifier.fillMaxSize().padding(padding)
        when (val t = terms) {
            null -> Centered(modifier) { CircularProgressIndicator() }
            else -> Column(
                modifier.verticalScroll(rememberScrollState()).padding(horizontal = 16.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                t.paragraphs.forEach { Text(it, style = MaterialTheme.typography.bodyLarge) }
            }
        }
    }
}
