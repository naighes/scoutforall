package io.github.naighes.scout4all

import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import java.io.File
import java.time.LocalTime
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.rules.TemporaryFolder
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class CrashesTest {
    @get:Rule val folder = TemporaryFolder()

    @Test
    fun theJournalKeepsTheLastOnes() {
        val journal = Journal(capacity = 2, now = { LocalTime.of(18, 0) })
        listOf("screen teams", "screen team/{id} id=1", "event ATTACK").forEach(journal::add)
        assertEquals(listOf("18:00 screen team/{id} id=1", "18:00 event ATTACK"), journal.lines())
    }

    @Test
    fun aCrashIsWrittenForTheNextStartThenForgotten() {
        val crashes = Crashes(folder.root)
        assertNull(crashes.last())
        crashes.write(IllegalStateException("boom"), listOf("18:00 undo"), "scout4all test")
        val report = crashes.last().orEmpty()
        assertTrue(report, report.startsWith("scout4all test\njava.lang.IllegalStateException: boom"))
        assertTrue(report, report.endsWith("Journal:\n18:00 undo"))
        crashes.forget()
        assertNull(crashes.last())
    }

    @Test
    fun aCrashThatCantBeWrittenIsLost() {
        val crashes = Crashes(File(folder.root, "not-there"))
        crashes.write(IllegalStateException("boom"), emptyList(), "scout4all test")
        assertNull(crashes.last())
    }
}
