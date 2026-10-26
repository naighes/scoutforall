package io.github.naighes.scout4all

import androidx.compose.foundation.layout.width
import androidx.compose.material3.PrimaryScrollableTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.ui.Modifier
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

/** The harness of the layout tests sees what's cut, and only that. */
@RunWith(RobolectricTestRunner::class)
class LayoutTest {
    @get:Rule(order = 0) val size = SizeRule(sizes.first { it.fontScale == 2f })
    @get:Rule(order = 1) val rule = createComposeRule()

    @Test
    fun aTextWithAnEllipsisIsCut() {
        rule.setContent { Text("Distribution", Modifier.width(40.dp), maxLines = 1, overflow = TextOverflow.Ellipsis) }
        assertEquals(listOf("text cut: Distribution"), rule.misfits(scrolls = true))
    }

    @Test
    fun theTabOfAScrollableRowIsWhole() {
        rule.setContent {
            PrimaryScrollableTabRow(selectedTabIndex = 0) {
                Tab(selected = true, onClick = {}, text = { Text("Distribution", maxLines = 1, overflow = TextOverflow.Ellipsis) })
            }
        }
        assertEquals(emptyList<String>(), rule.misfits(scrolls = true))
    }
}
