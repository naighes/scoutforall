package io.github.naighes.scout4all

import androidx.compose.ui.test.junit4.ComposeContentTestRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.test.core.app.ApplicationProvider
import io.github.naighes.scout4all.core.GenderEnum
import io.github.naighes.scout4all.core.Scout4All
import io.github.naighes.scout4all.core.TeamClassificationEnum
import io.github.naighes.scout4all.core.TeamForm
import kotlinx.coroutines.runBlocking

/** The app of the test: Robolectric gives each test a new one, with empty
 * data. */
val app: Scout4AllApplication get() = ApplicationProvider.getApplicationContext()

/** Calls the real core of the app, as the screens do. */
fun <T> core(call: (Scout4All) -> T): T = runBlocking { app.core(call) }

fun string(id: Int, vararg args: Any): String = app.getString(id, *args)

/** A team saved by the core; its id. */
fun saveTeam(name: String, year: Int = 2026): String = core {
    it.saveTeam(null, TeamForm(name, TeamClassificationEnum.REGIONAL, GenderEnum.WOMEN, year.toString()))
}

/** The screens read the core off the main thread, which the rule doesn't
 * wait for: until the text is there. */
fun ComposeContentTestRule.waitForText(text: String, substring: Boolean = false) =
    waitUntil(timeoutMillis = 5_000) {
        onAllNodesWithText(text, substring = substring).fetchSemanticsNodes().isNotEmpty()
    }
