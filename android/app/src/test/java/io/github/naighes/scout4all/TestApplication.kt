package io.github.naighes.scout4all

import androidx.work.testing.WorkManagerTestInitHelper
import io.github.naighes.scout4all.core.Scout4All
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers

/** The app, with a WorkManager that sends nothing: Robolectric creates the
 * app before the provider that would set it up. The core is called on the
 * main thread: in the tests of Compose a screen goes on where the core
 * answered, not on the main thread as on a phone, and a state written from
 * there may never show. Starting again only counts: the JVM of the tests
 * stays. Without the example team: the tests start from the data they
 * write. Without the log of the core: it is one for the JVM, each test
 * would move it into its own data. */
class TestApplication : Scout4AllApplication() {
    override val coreDispatcher: CoroutineDispatcher get() = Dispatchers.Main.immediate

    var startedAgain = 0

    override fun addExampleTeam(core: Scout4All) {}

    override fun startLog(dataDir: String) {}

    // a handler of every thread would outlive the test
    override fun catchCrashes() {}

    override fun startAgain() {
        startedAgain++
    }

    override fun onCreate() {
        WorkManagerTestInitHelper.initializeTestWorkManager(this)
        super.onCreate()
    }
}
