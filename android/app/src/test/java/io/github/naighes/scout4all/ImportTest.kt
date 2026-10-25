package io.github.naighes.scout4all

import android.content.Intent
import android.net.Uri
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import io.github.naighes.scout4all.core.TeamListing
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** An archive another app opens with this one ("Open with", Quick Share). */
@RunWith(RobolectricTestRunner::class)
class ImportTest {
    @get:Rule val rule = createEmptyComposeRule()

    /** The activity, opening the file with these bytes, past the terms. */
    private fun open(bytes: ByteArray): ActivityScenario<MainActivity> {
        core { it.acceptTerms() }
        val file = File(app.cacheDir, "opened.zip").apply { writeBytes(bytes) }
        val intent = Intent(Intent.ACTION_VIEW, Uri.fromFile(file)).setClass(app, MainActivity::class.java)
        return ActivityScenario.launch(intent)
    }

    /** The activity made again (another language, the dark theme, a split
     * window) while the archive waits to be confirmed: still there. */
    @Test
    fun theArchiveToImportOutlivesTheActivityMadeAgain() {
        // from another phone: not here
        val team = saveNationalTeam()
        val archive = core { it.exportTeam(team) }.bytes
        File(app.filesDir, "scout4all/$team").deleteRecursively()
        open(archive).use { scenario ->
            rule.waitForText(string(R.string.import_team_title))
            scenario.recreate()
            rule.waitForText(string(R.string.import_team_title))
            rule.onNode(hasText(string(R.string.import_archive)) and hasClickAction()).performClick()
            rule.waitFor { core { it.teams() }.filterIsInstance<TeamListing.Team>().size == 1 }
        }
    }

    /** As a sentence: the messages of the core start in lower case. */
    @Test
    fun whatWentWrongStartsWithACapitalLetter() {
        open("not a zip".toByteArray()).use {
            val raw = "the file is not the archive of a team or a match"
            assertEquals(raw.first().lowercaseChar(), raw.first())
            rule.waitForText(raw.replaceFirstChar { it.uppercaseChar() })
        }
    }
}
