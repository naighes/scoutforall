package io.github.naighes.scout4all

import android.content.Intent
import android.net.Uri
import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.test.core.app.ActivityScenario
import androidx.test.espresso.Espresso.pressBack
import io.github.naighes.scout4all.core.GenderEnum
import io.github.naighes.scout4all.core.MatchListing
import io.github.naighes.scout4all.core.TeamClassificationEnum
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

    /** The activity, sent the file with this text, past the terms; the
     * country of the new teams the one of the settings, when given. */
    private fun share(text: String = threeSets, country: String? = "AT"): ActivityScenario<MainActivity> {
        core { it.acceptTerms() }
        country?.let { c -> core { it.setDefaultCountry(c) } }
        val file = File(app.cacheDir, "match.dvw").apply { writeText(text) }
        val intent = Intent(Intent.ACTION_SEND)
            .setType("application/octet-stream")
            .putExtra(Intent.EXTRA_STREAM, Uri.fromFile(file))
            .setClass(app, MainActivity::class.java)
        return ActivityScenario.launch(intent)
    }

    private fun tap(text: String) =
        rule.onNode(hasText(text) and hasClickAction()).performClick()

    /** A field of the new team, in the part of the screen that scrolls. */
    private fun choose(text: String) =
        rule.onNode(hasText(text) and hasClickAction()).performScrollTo().performClick()

    private fun gender(gender: GenderEnum) = choose(core { it.genders() }.first { it.value == gender }.name)

    private fun classification(classification: TeamClassificationEnum) =
        choose(core { it.classifications() }.first { it.value == classification }.name)

    /** The new team filled in, as the form of a team wants it. */
    private fun fillNewTeam() {
        gender(GenderEnum.WOMEN)
        classification(TeamClassificationEnum.REGIONAL)
    }

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
            fillNewTeam()
            next()
            // team B: no team like it, only its name to start
            rule.waitForText(string(R.string.dvw_opponent_question, "Visiting"))
            next()
            rule.waitForText(string(R.string.dvw_ready, "Home", 3, 7))
            rule.waitForText(string(R.string.dvw_against_name, "Visiting"))
            rule.waitForText(string(R.string.dvw_competition, "Test"))
            tap(string(R.string.save))
            rule.waitForText(string(R.string.dvw_imported, "Home"))
            val team = teams().single()
            val matches = core { it.matches(team.id) }
            assertTrue(matches.single().let { it is MatchListing.Match && it.won.toInt() == 3 && it.opponent == "Visiting" })
            val form = core { it.team(team.id) }.form
            assertEquals(
                Triple(TeamClassificationEnum.REGIONAL, GenderEnum.WOMEN, "AT"),
                Triple(form.classification, form.gender, form.country),
            )
            tap(string(R.string.dvw_imported, "Home"))
            rule.waitForText("Name1 Surname1")
        }
    }

    /** Both teams, each on its own; the activity made again on the way
     * keeps where the import is. The home team against team B made with
     * the players of the file, back on team B going back to team A; the
     * visiting team into team B, against the home team proposed first:
     * two matches, two teams. */
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
            rule.waitForText(string(R.string.dvw_new_team_of, "Home", "2014"))
            fillNewTeam()
            next()
            rule.waitForText(string(R.string.dvw_opponent_question, "Visiting"))
            pressBack()
            rule.waitForText(string(R.string.dvw_new_team_of, "Home", "2014"))
            fillNewTeam()
            next()
            choose(string(R.string.dvw_opponent_new, "Visiting"))
            next()
            rule.waitForText(string(R.string.dvw_against_team, "Visiting", 7))
            tap(string(R.string.save))
            // the visiting team: like team B just made
            rule.waitForText(string(R.string.dvw_similar_found, "Visiting"))
            next()
            rule.waitForText(string(R.string.dvw_opponent_question, "Home"))
            rule.waitForText(string(R.string.dvw_same_name), substring = true)
            next()
            rule.waitForText(string(R.string.dvw_against_team, "Home", 0))
            tap(string(R.string.save))
            rule.waitForText(string(R.string.dvw_imported, "Visiting"))
            rule.waitForText(string(R.string.dvw_imported, "Home"))
            assertEquals(listOf("Home", "Visiting"), teams().map { it.name }.sorted())
            val home = teams().first { it.name == "Home" }
            assertEquals(2, core { it.matches(home.id) }.size)
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
            rule.waitForText(string(R.string.dvw_new_team_of, "Home", "2014"))
            fillNewTeam()
            next()
            rule.waitForText(string(R.string.dvw_opponent_question, "Visiting"))
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
            rule.waitForText(string(R.string.dvw_opponent_question, "Visiting"))
            next()
            rule.waitForText(string(R.string.dvw_already_saved, "Home"))
            tap(string(R.string.dvw_dont_import))
            rule.waitForText(string(R.string.dvw_skipped, "Home"))
            assertEquals(1, teams().size)
        }
    }

    /** The new team needs its level, gender and country, as the form of a
     * team: what's missing said next to its field, the import still on
     * the team; nothing saved until it's all there. */
    @Test
    fun aNewTeamNeedsLevelGenderAndCountry() {
        share(country = null).use {
            rule.waitForText(string(R.string.dvw_which_team))
            next()
            rule.waitForText(string(R.string.dvw_substitutions))
            next()
            rule.waitForText(string(R.string.dvw_new_team_of, "Home", "2014"))
            next()
            rule.waitForText("classification is required")
            classification(TeamClassificationEnum.REGIONAL)
            rule.onAllNodesWithText("classification is required").assertCountEquals(0)
            next()
            rule.waitForText("gender is required")
            gender(GenderEnum.WOMEN)
            next()
            rule.waitForText("country is required")
            rule.onNodeWithText(string(R.string.dvw_new_team_of, "Home", "2014")).assertExists()
            assertTrue(teams().isEmpty())
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
            rule.waitForText(string(R.string.dvw_new_team_of, "Home", "2014"))
            fillNewTeam()
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
