package io.github.naighes.scout4all

import android.os.Build
import android.view.View
import android.widget.FrameLayout
import android.widget.ImageView
import androidx.test.core.app.ApplicationProvider
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner
import org.robolectric.shadows.ShadowLooper

@RunWith(RobolectricTestRunner::class)
class SplashTest {
    private val context = ApplicationProvider.getApplicationContext<android.content.Context>()

    /** A splash as the system makes it: the ball, 240dp, in the middle. */
    private fun splash(): Pair<FrameLayout, View> {
        val size = (240 * context.resources.displayMetrics.density).toInt()
        val ball = View(context)
        val splash = FrameLayout(context).apply {
            addView(ball, FrameLayout.LayoutParams(size, size, android.view.Gravity.CENTER))
        }
        lay(splash)
        return splash to ball
    }

    /** What the window does to the splash on a phone of 1080x2400. */
    private fun lay(splash: View) {
        splash.measure(
            View.MeasureSpec.makeMeasureSpec(1080, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(2400, View.MeasureSpec.EXACTLY),
        )
        splash.layout(0, 0, 1080, 2400)
    }

    @Test
    fun theNameComesUnderTheBallThenAllFades() {
        val (splash, ball) = splash()
        splashExit(splash, ball).start()
        lay(splash)
        ShadowLooper.idleMainLooper(2, java.util.concurrent.TimeUnit.SECONDS)
        val name = (0 until splash.childCount).map(splash::getChildAt).filterIsInstance<ImageView>().single()
        // ball up, name under it with room between: Android 12 and later
        // draw the icon half bigger than its view
        assertTrue(ball.translationY < 0)
        val shown = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) 1.5f else 1f
        val middle = ball.top + ball.translationY + ball.height / 2f
        val radius = ball.height * shown * 84f / 288f
        val ballBottom = middle + radius + 16 * context.resources.displayMetrics.density
        assertTrue("name at ${name.top + name.translationY}, ball ends at $ballBottom", name.top + name.translationY > ballBottom)
        // and the two in the middle
        val top = middle - radius
        val bottom = name.top + name.translationY + name.height
        assertEquals(1200f, (top + bottom) / 2, 2f)
        assertEquals(1f, name.alpha)
        assertEquals(0f, splash.alpha)
    }

    @Test
    fun withoutTheBallTheSplashJustFades() {
        val (splash, _) = splash()
        splashExit(splash, null).start()
        ShadowLooper.idleMainLooper(2, java.util.concurrent.TimeUnit.SECONDS)
        assertEquals(1, splash.childCount)
        assertEquals(0f, splash.alpha)
    }
}
