package io.github.naighes.scout4all

import android.app.Application
import android.content.ComponentName
import android.content.Intent
import io.github.naighes.scout4all.core.InternalException
import io.github.naighes.scout4all.core.MobileException
import io.github.naighes.scout4all.core.Scout4All
import java.io.File
import java.util.Locale
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.withContext

/** Holds the core, one for the whole app. */
open class Scout4AllApplication : Application() {
    // the language chosen for the app, from the activity
    @Volatile private var language: String = Locale.getDefault().language

    // the same layout as ~/.scout4all, in the sandbox of the app
    private val lazyCore = lazy {
        val dataDir = File(filesDir, "scout4all").apply { mkdirs() }
        Scout4All(dataDir.path, language).also(::addExampleTeam)
    }

    /** A new installation starts with the example team, before the teams
     * are listed; without it the app works all the same. */
    protected open fun addExampleTeam(core: Scout4All) {
        try {
            core.addExampleTeamIfNew()
        } catch (_: MobileException) {
        }
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

    /** Where the core is called: it blocks. */
    protected open val coreDispatcher: CoroutineDispatcher get() = Dispatchers.IO

    /** Calls the core off the main thread: it blocks. */
    suspend fun <T> core(call: (Scout4All) -> T): T = onCore { call(core) }

    private val mutablePanic = MutableStateFlow<String?>(null)
    /** What a panic of the core said: its state may be broken, the app
     * starts again. */
    val panic: StateFlow<String?> = mutablePanic

    /** Calls what the core made (a scouting) where the core is called. A
     * panic of the core (UniFFI turns it into an exception) stops the
     * caller, as cancelled, and shows. */
    suspend fun <T> onCore(call: () -> T): T = withContext(coreDispatcher) {
        try {
            call()
        } catch (e: InternalException) {
            mutablePanic.value = e.message.orEmpty()
            throw CorePanic(e)
        }
    }

    /** A new process, from the teams: the core reads again what's saved. */
    open fun startAgain() {
        startActivity(Intent.makeRestartActivityTask(ComponentName(this, MainActivity::class.java)))
        Runtime.getRuntime().exit(0)
    }
}

/** A panic of the core: what called it stops, the app says so. */
class CorePanic(cause: InternalException) : CancellationException(cause.message) {
    init {
        initCause(cause)
    }
}

/** What the core says went wrong, for the user: a sentence, with the
 * capital letter the messages of the core leave to the app. */
val MobileException.reason: String
    get() = when (this) {
        is MobileException.Failed -> reason
        is MobileException.InvalidTeam -> reason
        is MobileException.InvalidPlayer -> reason
        is MobileException.InvalidMatch -> reason
        is MobileException.Scouting -> reason
    }.replaceFirstChar { it.titlecase(Locale.getDefault()) }
