package io.github.naighes.scout4all

import android.animation.Animator
import android.animation.AnimatorSet
import android.animation.ObjectAnimator
import android.graphics.Rect
import android.os.Build
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.view.animation.AccelerateInterpolator
import android.view.animation.DecelerateInterpolator
import android.widget.FrameLayout
import android.widget.ImageView

/** The ball of the splash (`splash_logo`): its radius, a fraction of the
 * icon. */
private const val BALL_RADIUS = 84f / 288f

/** How big the icon is drawn for its view: Android 12 and later show the
 * middle two thirds of it (192 of 288dp). */
private val ICON_SHOWN = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) 1.5f else 1f

/** Between the ball and the name, in dp. */
private const val GAP = 24f

/** Once the ball of the system has turned: the name comes under it, the two
 * together in the middle, a moment, then everything fades into the app.
 * Where the splash has no room for the name (not a layout) or no ball, just
 * the fade. */
internal fun splashExit(splash: View, ball: View?): Animator {
    val fade = ObjectAnimator.ofFloat(splash, View.ALPHA, 1f, 0f).apply {
        duration = 250
        interpolator = AccelerateInterpolator()
    }
    val name = (splash as? ViewGroup)?.takeIf { ball != null }?.let(::addName)
    return AnimatorSet().apply {
        when {
            ball == null || name == null -> play(fade)
            else -> {
                val density = splash.resources.displayMetrics.density
                val nameHeight = name.drawable.intrinsicHeight.toFloat()
                val seen = seen(ball, splash)
                // ball and name as one, in the middle of where the ball was
                val up = (GAP * density + nameHeight) / 2
                val nameMiddle = seen.exactCenterY() - up + seen.height() * ICON_SHOWN * BALL_RADIUS + GAP * density + nameHeight / 2
                val below = nameMiddle - splash.height / 2f
                val come = AnimatorSet().apply {
                    playTogether(
                        ObjectAnimator.ofFloat(ball, View.TRANSLATION_Y, 0f, -up),
                        ObjectAnimator.ofFloat(name, View.TRANSLATION_Y, below + nameHeight / 2, below),
                        ObjectAnimator.ofFloat(name, View.ALPHA, 0f, 1f),
                    )
                    duration = 250
                    interpolator = DecelerateInterpolator()
                }
                fade.startDelay = 300
                playSequentially(come, fade)
            }
        }
    }
}

/** Where `ball` is in `splash`, on the screen or, not there yet, as laid
 * out. */
private fun seen(ball: View, splash: View): Rect {
    val ballOnScreen = Rect()
    val splashOnScreen = Rect()
    return when (ball.getGlobalVisibleRect(ballOnScreen) && splash.getGlobalVisibleRect(splashOnScreen)) {
        true -> ballOnScreen.apply { offset(-splashOnScreen.left, -splashOnScreen.top) }
        false -> Rect(ball.left, ball.top, ball.right, ball.bottom)
    }
}

/** The name, in the middle of the splash, not seen yet. */
private fun addName(splash: ViewGroup): ImageView = ImageView(splash.context).apply {
    setImageResource(R.drawable.wordmark)
    alpha = 0f
    // the splash is gone in a moment: nothing for TalkBack
    importantForAccessibility = View.IMPORTANT_FOR_ACCESSIBILITY_NO
    splash.addView(
        this,
        FrameLayout.LayoutParams(
            ViewGroup.LayoutParams.WRAP_CONTENT,
            ViewGroup.LayoutParams.WRAP_CONTENT,
            Gravity.CENTER,
        ),
    )
}
