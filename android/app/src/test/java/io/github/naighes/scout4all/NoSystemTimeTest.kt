package io.github.naighes.scout4all

import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** Nothing reads the time by itself: the app asks its clock, the tests the
 * clock of the tests ([TestClock], [testDay]). The only reader is the clock
 * of the system; a stopwatch (`System.nanoTime`) is not "what time is it". */
class NoSystemTimeTest {
    private val reading = Regex(
        """\b(LocalDate|LocalDateTime|LocalTime|Instant|ZonedDateTime|OffsetDateTime|OffsetTime|YearMonth|Year|MonthDay)\s*\.\s*now\s*\(""" +
            """|System\s*\.\s*currentTimeMillis|\bjava\.util\.Date\s*\(\s*\)|Calendar\s*\.\s*getInstance\s*\(""",
    )

    private val systemClock = File("src/main/java/io/github/naighes/scout4all/Clock.kt")

    @Test
    fun onlyTheClockOfTheSystemReadsTheTime() {
        val sources = listOf("src/main/java", "src/test/java").map(::File)
        assertTrue("run from the module: ${File("").absolutePath}", sources.all(File::isDirectory) && systemClock.isFile)
        val readers = sources
            .flatMap { it.walk().filter { f -> f.extension == "kt" && f.canonicalFile != systemClock.canonicalFile }.toList() }
            .flatMap { f -> f.readLines().mapIndexedNotNull { i, line -> "${f.path}:${i + 1}: ${line.trim()}".takeIf { reading.containsMatchIn(line) } } }
        assertEquals("ask the clock (app.clock, testDay()) instead", emptyList<String>(), readers)
    }
}
