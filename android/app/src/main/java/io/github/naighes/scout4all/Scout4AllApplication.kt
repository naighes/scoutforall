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
    // the language chosen for the app, from the activity
    @Volatile private var language: String = Locale.getDefault().language

    // the same layout as ~/.scout4all, in the sandbox of the app
    private val lazyCore = lazy {
        val dataDir = File(filesDir, "scout4all").apply { mkdirs() }
        Scout4All(dataDir.path, language)
    }
    private val core: Scout4All by lazyCore

    // what wasn't sent last time (no network, the app closed)
    override fun onCreate() {
        super.onCreate()
        sendUploads()
    }

    /** The core follows the language of the app: the activity starts again
     * when the user changes it, the process may stay. Setting it doesn't
     * block, it can be called on the main thread. */
    fun followLanguage(language: String) {
        this.language = language
        if (lazyCore.isInitialized()) lazyCore.value.setLanguage(language)
    }

    /** Calls the core off the main thread: it blocks. */
    suspend fun <T> core(call: (Scout4All) -> T): T = withContext(Dispatchers.IO) { call(core) }
}

/** What the core says went wrong, for the user. */
val MobileException.reason: String
    get() = when (this) {
        is MobileException.Failed -> reason
        is MobileException.InvalidTeam -> reason
        is MobileException.InvalidPlayer -> reason
        is MobileException.InvalidMatch -> reason
        is MobileException.Scouting -> reason
    }
