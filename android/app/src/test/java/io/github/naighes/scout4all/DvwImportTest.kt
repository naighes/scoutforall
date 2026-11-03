package io.github.naighes.scout4all

import android.content.Intent
import android.net.Uri
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ActivityScenario
import androidx.test.espresso.Espresso.pressBack
import io.github.naighes.scout4all.core.GenderEnum
import io.github.naighes.scout4all.core.MatchListing
import io.github.naighes.scout4all.core.TeamListing
import java.io.File
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** A DataVolley file shared with the app (WhatsApp, Quick Share): imported
 * a step at a time, as the TUI. */
@RunWith(RobolectricTestRunner::class)
class DvwImportTest {
    @get:Rule val rule = createEmptyComposeRule()

    /** Three sets won by "Home" (players 1 to 7, the libero 7) against
     * "Visiting", on 2015-01-25: the fixture of the TUI. */
    private val threeSets: String =
        javaClass.classLoader!!.getResourceAsStream("three_sets.dvw")!!.use { it.readBytes() }.decodeToString()

    /** The activity, sent the file with this text, past the terms. */
    private fun share(text: String = threeSets): ActivityScenario<MainActivity> {
        core { it.acceptTerms() }
        val file = File(app.cacheDir, "match.dvw").apply { writeText(text) }
        val intent = Intent(Intent.ACTION_SEND)
            .setType("application/octet-stream")
            .putExtra(Intent.EXTRA_STREAM, Uri.fromFile(file))
            .setClass(app, MainActivity::class.java)
        return ActivityScenario.launch(intent)
    }

    private fun tap(text: String) =
        rule.onNode(hasText(text) and hasClickAction()).performClick()

    private fun next() = tap(string(R.string.dvw_next))

    private fun teams() = core { it.teams() }.filterIsInstance<TeamListing.Team>()

    /** The home team, new: then its page, with the match won. */
    @Test
    fun aNewTeamImported() {
        share().use {
            rule.waitForText(string(R.string.import_dvw))
            rule.waitForText("Home - Visiting")
            rule.waitForText(string(R.string.dvw_which_team))
            next()
            rule.waitForText(string(R.string.dvw_substitutions))
            // the rules of 2015: six
            rule.waitForText("6")
            next()
            rule.waitForText(string(R.string.dvw_new_team_of, "Home", "2014"))
            tap(core { it.genders() }.first { it.value == GenderEnum.WOMEN }.name)
            next()
            rule.waitForText(string(R.string.dvw_ready, "Home", 3, 7))
            tap(string(R.string.save))
            rule.waitForText(string(R.string.dvw_imported, "Home"))
            val team = teams().single()
            val matches = core { it.matches(team.id) }
            assertTrue(matches.single().let { it is MatchListing.Match && it.won.toInt() == 3 && it.opponent == "Visiting" })
            tap(string(R.string.dvw_imported, "Home"))
            rule.waitForText("Name1 Surname1")
        }
    }

    /** Both teams, each on its own; the activity made again on the way
     * keeps where the import is. */
    @Test
    fun bothTeamsImportedAcrossTheActivityMadeAgain() {
        share().use { scenario ->
            rule.waitForText(string(R.string.dvw_both))
            tap(string(R.string.dvw_both))
            next()
            rule.waitForText(string(R.string.dvw_substitutions))
            scenario.recreate()
            rule.waitForText(string(R.string.dvw_substitutions))
            next()
            next()
            tap(string(R.string.save))
            rule.waitForText(string(R.string.dvw_new_team_of, "Visiting", "2014"))
            next()
            tap(string(R.string.save))
            rule.waitForText(string(R.string.dvw_imported, "Visiting"))
            rule.waitForText(string(R.string.dvw_imported, "Home"))
            assertEquals(listOf("Home", "Visiting"), teams().map { it.name }.sorted())
        }
    }

    /** Imported again: the team like it proposed, the match overwritten
     * only when the user says so. */
    @Test
    fun aMatchSavedAlready() {
        share().use {
            rule.waitForText(string(R.string.dvw_which_team))
            next()
            rule.waitForText(string(R.string.dvw_substitutions))
            next()
            next()
            tap(string(R.string.save))
            rule.waitForText(string(R.string.dvw_imported, "Home"))
        }
        share().use {
            rule.waitForText(string(R.string.dvw_which_team))
            next()
            rule.waitForText(string(R.string.dvw_substitutions))
            next()
            rule.waitForText(string(R.string.dvw_similar_found, "Home"))
            rule.waitForText(string(R.string.dvw_same_name), substring = true)
            next()
            rule.waitForText(string(R.string.dvw_already_saved, "Home"))
            tap(string(R.string.dvw_dont_import))
            rule.waitForText(string(R.string.dvw_skipped, "Home"))
            assertEquals(1, teams().size)
        }
    }

    /** Sets that don't add up: not imported, the reason said. */
    @Test
    fun aMatchNotTranslatedSaysWhy() {
        share(threeSets.replaceFirst("True;;;;25-0;;", "True;;;;25-3;;")).use {
            rule.waitForText(string(R.string.dvw_which_team))
            next()
            rule.waitForText(string(R.string.dvw_substitutions))
            next()
            next()
            rule.waitForText("25-3 in the file, 25-0 from its rallies", substring = true)
            assertTrue(teams().isEmpty())
        }
    }

    /** Back goes back a step, and from the first one to the teams. */
    @Test
    fun backGoesBackAStep() {
        share().use {
            rule.waitForText(string(R.string.dvw_which_team))
            next()
            rule.waitForText(string(R.string.dvw_substitutions))
            rule.onNode(hasText(string(R.string.dvw_substitutions))).assertExists()
            pressBack()
            rule.waitForText(string(R.string.dvw_which_team))
            pressBack()
            rule.waitForText(string(R.string.app_name))
        }
    }
}
