package io.github.naighes.scout4all

import android.content.Context
import android.graphics.Bitmap
import androidx.test.core.app.ApplicationProvider
import io.github.naighes.scout4all.core.PhaseEnum
import io.github.naighes.scout4all.core.VideoStanding
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.io.ByteArrayOutputStream

/** How the match stands over the video made has the logo of a team before
 * its name, the initials of the one without it. */
@RunWith(RobolectricTestRunner::class)
class MontageBoardTest {
    private val red = Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888).let { bitmap ->
        bitmap.eraseColor(android.graphics.Color.RED)
        ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
    }

    /** The red pixels of each of the two lines. */
    private fun redByLine(emblems: List<Emblem>): List<Int> {
        val bitmap = boardBitmap(
            ApplicationProvider.getApplicationContext<Context>(),
            MontageTexts("Set %d", "SIDE-OUT", "BREAK"),
            listOf("Cortona Volley", "Polisportiva Savinese"),
            emblems,
            VideoStanding(2u, 1u, 0u, 12u, 10u, "P3", PhaseEnum.SIDE_OUT),
        )
        return listOf(0, bitmap.height / 2).map { top ->
            (top until top + bitmap.height / 2).sumOf { y ->
                (0 until bitmap.width / 2).count { x -> bitmap.getPixel(x, y) == android.graphics.Color.RED }
            }
        }
    }

    @Test
    fun theLogosOverTheVideoMade() {
        val none = emblemOf("no image".toByteArray(), "Polisportiva Savinese")
        assertEquals(Emblem.Initials("PS"), none)
        val (first, second) = redByLine(listOf(emblemOf(red, "Cortona Volley"), none))
        assertTrue("$first", first > 900)
        assertEquals(0, second)
        assertEquals(listOf(0, 0), redByLine(listOf(emblemOf(null, "Cortona Volley"), none)))
    }
}
