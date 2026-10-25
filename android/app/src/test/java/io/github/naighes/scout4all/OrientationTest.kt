package io.github.naighes.scout4all

import android.content.pm.ActivityInfo
import androidx.test.core.app.ActivityScenario
import org.junit.Assert.assertEquals
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.annotation.Config

/** A phone stays upright, a tablet turns. */
@RunWith(RobolectricTestRunner::class)
class OrientationTest {
    private fun orientation(): Int = ActivityScenario.launch(MainActivity::class.java).use { scenario ->
        var requested = 0
        scenario.onActivity { requested = it.requestedOrientation }
        requested
    }

    @Test
    @Config(qualifiers = "w360dp-h800dp")
    fun aPhoneStaysUpright() = assertEquals(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT, orientation())

    @Test
    @Config(qualifiers = "w800dp-h360dp-land")
    fun aPhoneTurnedStaysUpright() = assertEquals(ActivityInfo.SCREEN_ORIENTATION_PORTRAIT, orientation())

    @Test
    @Config(qualifiers = "w800dp-h1280dp")
    fun aTabletTurns() = assertEquals(ActivityInfo.SCREEN_ORIENTATION_UNSPECIFIED, orientation())
}
