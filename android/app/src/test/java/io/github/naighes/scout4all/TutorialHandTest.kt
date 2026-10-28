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

    /** The ring grows from the tip, not from the middle of the target,
     * where the words are: up to v0.10 it went over them. */
    @Test
    fun theRingIsAtTheTipOfTheFinger() {
        val margin = 18f
        val finger = 120
        val hole = Rect(100f, 400f, 500f, 580f)
        val tip = tipAt(hole, margin)
        val at = fingerAt(hole, margin, finger)
        assertTrue("the ring at $tip, the finger at $at", tip.x.toInt() == at.x + finger * 11 / 24 && tip.y.toInt() == at.y + finger / 8)
        assertTrue("the ring at ${tip.y}, the middle at ${hole.center.y}", tip.y > hole.center.y + 36)
    }
}
