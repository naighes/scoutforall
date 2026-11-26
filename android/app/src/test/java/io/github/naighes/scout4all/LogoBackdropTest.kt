package io.github.naighes.scout4all

import android.graphics.Bitmap
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.unit.dp
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import java.io.ByteArrayOutputStream

/** A logo that would vanish on the theme (always dark) gets a light
 * backdrop; one that shows on it gets none (TODO/011 R8.6). */
@RunWith(RobolectricTestRunner::class)
class LogoBackdropTest {
    @get:Rule val rule = createComposeRule()

    /** A square of `color`, its right half transparent, as PNG. */
    private fun logo(color: Int): ByteArray {
        val bitmap = Bitmap.createBitmap(8, 8, Bitmap.Config.ARGB_8888)
        for (x in 0 until 4) for (y in 0 until 8) bitmap.setPixel(x, y, color)
        return ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.PNG, 100, it) }.toByteArray()
    }

    private fun count(tag: String) = rule.onAllNodesWithTag(tag, useUnmergedTree = true).fetchSemanticsNodes().size

    /** Whether the logo is drawn on a backdrop, once its tone is read. */
    private fun backdrop(bytes: ByteArray): Boolean {
        rule.setContent { Scout4AllTheme { TeamLogoBox(bytes, 48.dp) } }
        rule.waitFor { count(LOGO_BACKDROP) + count(LOGO_AS_IT_IS) == 1 }
        assert(count(SHOWN_IMAGE) == 1)
        return count(LOGO_BACKDROP) == 1
    }

    @Test
    fun aBlackLogoGetsALightBackdrop() {
        assert(backdrop(logo(0xFF000000.toInt())))
    }

    @Test
    fun aWhiteLogoShowsAsItIs() {
        assert(!backdrop(logo(0xFFFFFFFF.toInt())))
    }

    @Test
    fun aRedLogoShowsAsItIs() {
        assert(!backdrop(logo(0xFFC81414.toInt())))
    }
}
