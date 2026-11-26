package io.github.naighes.scout4all

import android.content.Context
import android.telephony.TelephonyManager
import androidx.compose.ui.test.assertIsNotEnabled
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.hasScrollToNodeAction
import androidx.compose.ui.test.performScrollToNode
import io.github.naighes.scout4all.core.VideoSeconds
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf

@RunWith(RobolectricTestRunner::class)
class SettingsTest {
    @get:Rule val rule = createComposeRule()

    @Test
    fun theStatisticsAreSentUntilTurnedOff() {
        val opened = mutableListOf<String>()
        rule.setContent {
            Scout4AllTheme {
                SettingsScreen(onBack = {}, onTerms = { opened += "terms" }, onAbout = { opened += "about" })
            }
        }
        rule.waitForText(string(R.string.send_statistics))
        assertEquals(true, core { it.analyticsEnabled() })
        rule.onNode(hasText(string(R.string.send_statistics)) and hasClickAction()).performClick()
        rule.waitFor { !core { it.analyticsEnabled() } }
        rule.onNode(hasText(string(R.string.send_statistics)) and hasClickAction()).performClick()
        rule.waitFor { core { it.analyticsEnabled() } }
        rule.onNode(hasText(string(R.string.terms_of_use)) and hasClickAction()).performClick()
        rule.onNode(hasText(string(R.string.about)) and hasClickAction()).performClick()
        assertEquals(listOf("terms", "about"), opened)
    }

    @Test
    fun theLicensesOfOthersOpenFromTheAboutAndCloseWithTheArrow() {
        val opened = mutableListOf<String>()
        rule.setContent { Scout4AllTheme { AboutScreen(onBack = {}, onLicenses = { opened += "licenses" }) } }
        rule.onNode(hasText(string(R.string.third_party_licenses)) and hasClickAction()).performClick()
        assertEquals(listOf("licenses"), opened)
    }
}

/** The licenses of the parts of others: each with its text, closed with the
 * arrow back; the terms read from the settings close the same way. */
@RunWith(RobolectricTestRunner::class)
class LicensesTest {
    @get:Rule val rule = createComposeRule()

    @Test
    fun eachPartWithItsLicenseClosedWithTheArrow() {
        var closed = false
        rule.setContent { Scout4AllTheme { LicensesScreen(onBack = { closed = true }) } }
        rule.waitForText("SeetaFace Engine")
        val list = rule.onNode(hasScrollToNodeAction())
        list.performScrollToNode(hasText("Redistributions in binary form", substring = true))
        for (part in listOf("U-2-Net (U2NetP)", "Source Sans 3", "Rust crates, Android libraries")) {
            list.performScrollToNode(hasText(part))
        }
        // the libraries, their thousands of lines in paragraphs
        list.performScrollToNode(hasText("- serde ", substring = true))
        list.performScrollToNode(hasText("Permission is hereby granted", substring = true))
        rule.onNodeWithContentDescription(string(R.string.back)).performClick()
        assertEquals(true, closed)
    }

    @Test
    fun theTermsFromTheSettingsCloseWithTheArrow() {
        var closed = false
        rule.setContent { Scout4AllTheme { TermsScreen(onAccept = null, onBack = { closed = true }) } }
        rule.waitForText(core { it.terms() }.paragraphs.first())
        rule.onNodeWithContentDescription(string(R.string.back)).performClick()
        assertEquals(true, closed)
    }
}

/** The seconds of the video: one more or less, saved at once, in their
 * bounds. */
@RunWith(RobolectricTestRunner::class)
class VideoSecondsTest {
    @get:Rule val rule = createComposeRule()

    @Test
    fun oneSecondMoreOrLessInTheirBounds() {
        rule.setContent { Scout4AllTheme { SettingsScreen(onBack = {}, onTerms = {}, onAbout = {}) } }
        val step = string(R.string.video_step)
        val lead = string(R.string.video_lead)
        rule.waitForText(step)
        val more = { name: String -> rule.onNodeWithContentDescription(string(R.string.video_more) + " " + name) }
        val less = { name: String -> rule.onNodeWithContentDescription(string(R.string.video_less) + " " + name) }
        more(step).performClick()
        rule.waitFor { core { it.videoSeconds() } == VideoSeconds(6u, 2u) }
        less(lead).performClick()
        rule.waitFor { core { it.videoSeconds() } == VideoSeconds(6u, 1u) }
        less(lead).performClick()
        rule.waitFor { core { it.videoSeconds() }.leadS == 0u }
        less(lead).assertIsNotEnabled()
        rule.waitForText("6 s")
    }
}

/** The country of a new team: guessed from the phone, chosen in the
 * settings. */
@RunWith(RobolectricTestRunner::class)
class DefaultCountryTest {
    @get:Rule val rule = createComposeRule()

    private fun show() {
        rule.setContent { Scout4AllTheme { SettingsScreen(onBack = {}, onTerms = {}, onAbout = {}) } }
        rule.waitForText(string(R.string.default_country))
    }

    @Test
    fun chosenFromTheList() {
        show()
        rule.waitForText(string(R.string.default_country_none))
        rule.onNode(hasText(string(R.string.default_country)) and hasClickAction()).performClick()
        rule.onNode(hasSetTextAction() and hasText(string(R.string.search_country))).performTextInput("japa")
        rule.waitUntil(5_000) { rule.onAllNodes(hasText("Japan") and hasClickAction()).fetchSemanticsNodes().size == 1 }
        rule.onNode(hasText("Japan") and hasClickAction()).performClick()
        rule.waitFor { core { it.defaultCountry() }?.value == "JP" }
        rule.waitForText("🇯🇵  Japan")
    }

    @Test
    fun guessedFromTheSimFirst() {
        val telephony = app.getSystemService(Context.TELEPHONY_SERVICE) as TelephonyManager
        shadowOf(telephony).setSimCountryIso("br")
        shadowOf(telephony).setNetworkCountryIso("ar")
        assertEquals("br", app.countryHints().first())
        core { it.guessDefaultCountry(app.countryHints()) }
        assertEquals("BR", core { it.defaultCountry() }?.value)
        // once: the SIM of another country changes nothing
        shadowOf(telephony).setSimCountryIso("it")
        core { it.guessDefaultCountry(app.countryHints()) }
        assertEquals("BR", core { it.defaultCountry() }?.value)
    }

    @Test
    fun withoutSimTheLocale() {
        assertEquals(listOf("", "", "en-US"), app.countryHints())
        core { it.guessDefaultCountry(app.countryHints()) }
        assertEquals("US", core { it.defaultCountry() }?.value)
    }
}

/** The terms at the first start, then the teams; not again. */
@RunWith(RobolectricTestRunner::class)
class FirstStartTest {
    @get:Rule val rule = createAndroidComposeRule<MainActivity>()

    @Test
    fun theTermsThenTheTeams() {
        rule.waitForText(string(R.string.accept_terms))
        assertEquals(false, core { it.termsSeen() })
        rule.onNode(hasText(string(R.string.accept_terms)) and hasClickAction()).performClick()
        rule.waitForText(string(R.string.no_teams))
        assertEquals(true, core { it.termsSeen() })
        // started again: straight to the teams
        rule.activityRule.scenario.recreate()
        rule.waitForText(string(R.string.no_teams))
        assertEquals(0, rule.onAllNodes(hasText(string(R.string.accept_terms))).fetchSemanticsNodes().size)
    }
}
