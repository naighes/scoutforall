package io.github.naighes.scout4all

import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onFirst
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The gyms on the phone: written in their form, chosen in the form of a
 * session, asked about when another team holds the same one. */
@RunWith(RobolectricTestRunner::class)
class VenuesTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveNationalTeam()

    private fun gym(name: String) = core { it.createVenueNamed(name) }.id

    private fun chooseGym(name: String) {
        rule.onAllNodes(hasText(string(R.string.venue))).onFirst().performScrollTo().performClick()
        rule.waitForText(name)
        rule.onNode(hasText(name) and hasClickAction()).performClick()
        rule.waitForIdle()
    }

    @Test
    fun aGymIsWrittenInItsForm() {
        val saved = mutableListOf<Unit>()
        rule.setContent { Scout4AllTheme { VenueFormScreen(null, onBack = {}, onSaved = { saved += Unit }) } }
        rule.waitForText(string(R.string.venue_name))
        rule.type(string(R.string.venue_name), "Terontola")
        rule.type(string(R.string.venue_map_link), "https://www.google.com/maps/@43.2,12.0,15z")
        rule.save()
        rule.waitFor({ "not saved" }) { saved.isNotEmpty() }
        val v = core { it.venues() }.single()
        assertEquals("Terontola", v.name)
        // the coordinates come from the link
        assertEquals(43.2, v.latitude!!, 0.0001)
        assertEquals(12.0, v.longitude!!, 0.0001)
    }

    @Test
    fun aNameIsAskedFor() {
        rule.setContent { Scout4AllTheme { VenueFormScreen(null, onBack = {}, onSaved = {}) } }
        rule.waitForText(string(R.string.venue_name))
        rule.save()
        rule.assertInvalid(string(R.string.venue_name))
        assertEquals(0, core { it.venues() }.size)
    }

    @Test
    fun theGymOfASessionIsChosenInItsForm() {
        val id = gym("Terontola")
        val saved = mutableListOf<String>()
        rule.setContent { Scout4AllTheme { SessionFormScreen(team, null, onBack = {}, onSaved = { saved += it }) } }
        rule.waitForText(string(R.string.tr_session_duration))
        chooseGym("Terontola")
        rule.save()
        rule.waitFor({ "not saved" }) { saved.isNotEmpty() }
        assertEquals(id, core { it.trainingSession(saved.single()) }.venueId)
    }

    @Test
    fun aGymHeldByAnotherTeamAsksBeforeSaving() {
        val id = gym("Terontola")
        val other = saveTeam("Under 19")
        val day = testDay().plusDays(1)
        core { it.saveSession(other, null, it.newSessionDraft().copy(start = "${day}T18:00", duration = "90", venue = id)) }
        val saved = mutableListOf<String>()
        rule.setContent { Scout4AllTheme { SessionFormScreen(team, null, onBack = {}, onSaved = { saved += it }, day = day) } }
        rule.waitForText(string(R.string.tr_session_duration))
        chooseGym("Terontola")
        rule.save()
        // asked, nothing written
        rule.waitForText(string(R.string.venue_save_anyway))
        assertTrue(saved.isEmpty())
        assertEquals(0, core { it.trainingSessions(team) }.size)
        rule.onNode(hasText(string(R.string.venue_save_anyway)) and hasClickAction()).performClick()
        rule.waitFor({ "not saved" }) { saved.isNotEmpty() }
        assertEquals(id, core { it.trainingSession(saved.single()) }.venueId)
    }

    @Test
    fun aFreeGymDoesNotAsk() {
        val id = gym("Terontola")
        val saved = mutableListOf<String>()
        rule.setContent { Scout4AllTheme { SessionFormScreen(team, null, onBack = {}, onSaved = { saved += it }) } }
        rule.waitForText(string(R.string.tr_session_duration))
        chooseGym("Terontola")
        rule.save()
        rule.waitFor({ "asked or not saved" }) { saved.isNotEmpty() }
        assertEquals(id, core { it.trainingSession(saved.single()) }.venueId)
    }

    @Test
    fun theGymsAreListedPinnedFirst() {
        val a = gym("Arezzo")
        gym("Terontola")
        core { it.setVenuePinned(a, false) }
        val z = gym("Zagabria")
        core { it.setVenuePinned(z, true) }
        rule.setContent { Scout4AllTheme { VenuesScreen(onBack = {}, onVenue = {}, onNew = {}) } }
        rule.waitForText("Zagabria")
        // the initials at the head of each row
        listOf("Z" to "Zagabria", "A" to "Arezzo", "T" to "Terontola").forEach { (initials, name) ->
            rule.onNode(hasText(initials) and hasText(name) and hasClickAction()).assertExists()
        }
        assertEquals(listOf("Zagabria", "Arezzo", "Terontola"), core { it.venues() }.map { it.name })
        assertNull(core { it.venue(a) }.latitude)
    }
}
