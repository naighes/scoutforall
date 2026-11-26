package io.github.naighes.scout4all

import androidx.compose.runtime.Composable
import androidx.compose.ui.platform.LocalContext
import io.github.naighes.scout4all.core.Clock
import java.time.Duration
import java.time.Instant
import java.time.LocalDate
import java.time.LocalDateTime
import java.time.OffsetDateTime
import java.time.ZoneOffset

/** The clock of the system: the only place the app reads the time; the
 * rest asks [Scout4AllApplication.clock], which the tests stop. */
object SystemClock : Clock {
    override fun now(): LocalDateTime = LocalDateTime.now()

    override fun utcNow(): Instant = Instant.now()
}

/** Asks [clock] each time: the core keeps the one it was made with, a test
 * may change the app's later. */
internal class AskingClock(private val clock: () -> Clock) : Clock {
    override fun now(): LocalDateTime = clock().now()

    override fun utcNow(): Instant = clock().utcNow()
}

fun Clock.today(): LocalDate = now().toLocalDate()

/** Where the app runs, from UTC: the wall and UTC apart, to the minute (a
 * fixed clock is a zone of its own), as the core works it out. */
fun Clock.offset(): ZoneOffset {
    val seconds = Duration.between(utcNow().atOffset(ZoneOffset.UTC).toLocalDateTime(), now()).seconds
    // the two asked a moment apart
    val minutes = Math.floorDiv(seconds + 30, 60L)
    return runCatching { ZoneOffset.ofTotalSeconds((minutes * 60).toInt()) }.getOrDefault(ZoneOffset.UTC)
}

/** Now, with where the app runs. */
fun Clock.offsetNow(): OffsetDateTime = now().atOffset(offset())

/** The clock of the app, for a screen. */
@Composable
fun clock(): Clock = (LocalContext.current.applicationContext as Scout4AllApplication).clock
