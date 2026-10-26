package io.github.naighes.scout4all

import androidx.compose.ui.geometry.Rect
import org.junit.Assert.assertTrue
import org.junit.Test

/** The finger of the tutorial leaves the words of the target in sight. */
class TutorialHandTest {
    @Test
    fun theFingerIsBelowTheMiddleOfTheTarget() {
        val margin = 18f
        val finger = 120
        // a button 144 high, 18 around it
        val hole = Rect(100f, 400f, 500f, 580f)
        val at = fingerAt(hole, margin, finger)
        assertTrue("the finger from ${at.y}, the middle at ${hole.center.y}", at.y > hole.center.y + 36)
        assertTrue("the finger from ${at.y}, the target ends at ${hole.bottom - margin}", at.y < hole.bottom - margin)
        assertTrue("the tip off the middle: ${at.x + finger * 11 / 24}", at.x + finger * 11 / 24 == hole.center.x.toInt())
    }
}
