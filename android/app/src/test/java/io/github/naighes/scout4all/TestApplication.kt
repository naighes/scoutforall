package io.github.naighes.scout4all

import androidx.work.testing.WorkManagerTestInitHelper
import io.github.naighes.scout4all.core.Clock
import io.github.naighes.scout4all.core.Scout4All
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.ZoneId
import java.time.ZoneOffset

/** The app, with a WorkManager that sends nothing: Robolectric creates the
 * app before the provider that would set it up. The core is called on the
 * main thread: in the tests of Compose a screen goes on where the core
 * answered, not on the main thread as on a phone, and a state written from
 * there may never show. Starting again only counts: the JVM of the tests
 * stays. Without the example team: the tests start from the data they
 * write. Without the log of the core: it is one for the JVM, each test
 * would move it into its own data. */
class TestApplication : Scout4AllApplication() {
    // never the system's: a test may stop it (the core asks this one too)
    override var clock: Clock = TestClock()

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

/** Where the clock of the tests starts: noon of a Thursday, the same day as
 * the one of the Rust tests. */
val TEST_START: LocalDateTime = LocalDateTime.of(2026, 11, 26, 12, 0)

/** The clock of the tests: from [start] on the wall of the zone the tests
 * run in, going on as a stopwatch. The day never changes under a test
 * (whatever day it is run), and two moments still come one after another. */
class TestClock(start: LocalDateTime = TEST_START) : Clock {
    private val zone = ZoneId.systemDefault()
    private val from = start.atZone(zone).toInstant()
    private val started = System.nanoTime()

    override fun utcNow(): Instant = from.plusNanos(System.nanoTime() - started)

    override fun now(): LocalDateTime = LocalDateTime.ofInstant(utcNow(), zone)
}

/** Today for a test: the day of the clock of the app, never the system's. */
fun testDay(): LocalDate = app.clock.today()

/** A clock stopped at `now` on the wall, `offset` from UTC. */
class FixedClock(private val now: LocalDateTime, private val offset: ZoneOffset) : Clock {
    override fun now(): LocalDateTime = now

    override fun utcNow(): Instant = now.toInstant(offset)
}
