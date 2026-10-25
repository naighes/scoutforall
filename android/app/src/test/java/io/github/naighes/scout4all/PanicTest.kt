package io.github.naighes.scout4all

import android.content.Intent
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.junit4.createAndroidComposeRule
import androidx.compose.ui.test.performClick
import androidx.lifecycle.lifecycleScope
import io.github.naighes.scout4all.core.InternalException
import kotlinx.coroutines.launch
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.Shadows.shadowOf

/** A panic of the core, as UniFFI turns it into an exception: the app
 * doesn't close, it says so and starts again. */
@RunWith(RobolectricTestRunner::class)
class PanicTest {
    @get:Rule val rule = createAndroidComposeRule<MainActivity>()

    private fun panic() {
        rule.runOnUiThread {
            // as a screen calls the core: an exception not caught would
            // close the app
            rule.activity.lifecycleScope.launch { app.core { throw InternalException("index out of bounds") } }
        }
        rule.waitForText(string(R.string.panic_title))
    }

    @Test
    fun aPanicOfTheCoreIsReported() {
        rule.waitForText(string(R.string.accept_terms))
        panic()
        rule.onNode(hasText(string(R.string.report)) and hasClickAction()).performClick()
        val mail = shadowOf(app).nextStartedActivity
        assertEquals(Intent.ACTION_SENDTO, mail.action)
        assertTrue(mail.getStringExtra(Intent.EXTRA_TEXT).orEmpty().contains("index out of bounds"))
        // the dialog stays until the app starts again
        rule.waitForText(string(R.string.panic_title))
    }

    @Test
    fun aPanicOfTheCoreStartsTheAppAgain() {
        rule.waitForText(string(R.string.accept_terms))
        panic()
        rule.onNode(hasText(string(R.string.start_again)) and hasClickAction()).performClick()
        rule.waitForIdle()
        assertEquals(1, (app as TestApplication).startedAgain)
    }
}
