package io.github.naighes.scout4all

import android.content.Context
import android.telephony.TelephonyManager
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.performClick
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
