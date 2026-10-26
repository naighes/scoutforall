package io.github.naighes.scout4all

import java.io.File
import java.io.IOException
import java.time.LocalTime
import java.time.temporal.ChronoUnit

/** The last things done in the app, to reproduce a crash: the screens with
 * their ids and the choices of scouting, never a name. In memory, the
 * oldest going; written only with a crash. */
class Journal(private val capacity: Int = 100, private val now: () -> LocalTime = LocalTime::now) {
    private val entries = ArrayDeque<String>()

    fun add(what: String) = synchronized(entries) {
        entries.addLast("${now().truncatedTo(ChronoUnit.MILLIS)} $what")
        while (entries.size > capacity) entries.removeFirst()
    }

    fun lines(): List<String> = synchronized(entries) { entries.toList() }
}

/** The report of the last crash, in the folder of the app, for the next
 * start to offer to send it. */
class Crashes(private val dir: File) {
    private val file get() = File(dir, "crash.txt")

    /** What helps to reproduce it: the phone, the error, the journal. A
     * report that can't be written is lost, the crash goes on. */
    fun write(error: Throwable, journal: List<String>, device: String) = try {
        file.writeText(
            listOf(device, error.stackTraceToString().take(MAX_STACK), "Journal:", *journal.toTypedArray())
                .joinToString("\n"),
        )
    } catch (_: IOException) {
    }

    /** The report of the last crash, if one is there. */
    fun last(): String? = try {
        file.takeIf { it.exists() }?.readText()
    } catch (_: IOException) {
        null
    }

    fun forget() {
        file.delete()
    }

    /** From now on, a crash of any thread is written before the system
     * closes the app (and Play learns of it). */
    fun catchAll(journal: Journal, device: () -> String) {
        val system = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, error ->
            write(error, journal.lines(), device())
            system?.uncaughtException(thread, error)
        }
    }
}

/** An e-mail can't hold all of a long stack: its start is what counts. */
private const val MAX_STACK = 8_000
