package io.github.naighes.scout4all

import android.app.Application
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Scout4All
import java.io.File
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Holds the core, one for the whole app. */
class Scout4AllApplication : Application() {
    // the same layout as ~/.scout4all, in the sandbox of the app
    private val core: Scout4All by lazy {
        val dataDir = File(filesDir, "scout4all").apply { mkdirs() }
        Scout4All(dataDir.path, Locale.getDefault().language)
    }

    /** Calls the core off the main thread: it blocks. */
    suspend fun <T> core(call: (Scout4All) -> T): T = withContext(Dispatchers.IO) { call(core) }
}

/** What the core says went wrong, for the user. */
val MobileException.reason: String
    get() = when (this) {
        is MobileException.Failed -> reason
        is MobileException.InvalidTeam -> reason
    }
