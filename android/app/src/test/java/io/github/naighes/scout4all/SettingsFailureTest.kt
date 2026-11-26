package io.github.naighes.scout4all

import android.content.Context
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** A setting that can't be saved is told in a dialog and stays as it
 * was. */
@RunWith(RobolectricTestRunner::class)
class SettingsFailureTest {
    @get:Rule val rule = createComposeRule()

    @Test
    fun theStatisticsNotSavedAreToldInADialog() {
        rule.setContent { Scout4AllTheme { SettingsScreen(onBack = {}, onTerms = {}, onAbout = {}) } }
        rule.waitForText(string(R.string.send_statistics))
        val files = java.io.File(ApplicationProvider.getApplicationContext<Context>().filesDir, "scout4all")
        // nothing can be written next to the config
        files.setWritable(false)
        try {
            rule.onNode(hasText(string(R.string.send_statistics)) and hasClickAction()).performClick()
            rule.waitFor { rule.onAllNodesWithTag(NOTICE_TAG).fetchSemanticsNodes().isNotEmpty() }
        } finally {
            files.setWritable(true)
        }
        assertEquals(true, core { it.analyticsEnabled() })
        rule.onNode(hasText(string(R.string.ok)) and hasClickAction()).performClick()
        rule.onNodeWithTag(NOTICE_TAG).assertDoesNotExist()
    }
}
