package io.github.naighes.scout4all

import androidx.compose.ui.test.assertTextContains
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.getUnclippedBoundsInRoot
import androidx.compose.ui.unit.dp
import kotlin.math.abs
import androidx.compose.ui.test.junit4.createComposeRule
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import io.github.naighes.scout4all.core.PlayerForm
import io.github.naighes.scout4all.core.RoleEnum
import org.junit.Assert.assertEquals
import java.time.format.FormatStyle
import java.time.format.DateTimeFormatter
import java.time.LocalDate
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import org.robolectric.RobolectricTestRunner

@RunWith(RobolectricTestRunner::class)
class PlayerFormTest {
    @get:Rule val rule = createComposeRule()

    private val team = saveTeam("Italia")
    private var done = 0

    private fun show(player: String? = null) {
        rule.setContent { Scout4AllTheme { PlayerFormScreen(team, player, onBack = {}, onDone = { done++ }) } }
        rule.waitForText(string(R.string.number))
    }

    private fun players() = core { it.team(team) }.players

    private fun role(role: RoleEnum) = core { it.roles() }.first { it.value == role }.name

    @Test
    fun aPlayerWithoutNumberIsNotSaved() {
        show()
        rule.type(string(R.string.name), "Paola Egonu")
        // the first number free written: taken away
        rule.type(string(R.string.number), "")
        rule.onNodeWithText(role(RoleEnum.OPPOSITE_HITTER)).performScrollTo().performClick()
        rule.save()
        rule.assertInvalid(string(R.string.number))
        assertEquals(0, done)
        assertEquals(0, players().size)
    }

    @Test
    fun aNumberTakenIsRejected() {
        core { it.savePlayer(team, null, PlayerForm("Alessia Orro", RoleEnum.SETTER, "8")) }
        show()
        rule.type(string(R.string.name), "Paola Egonu")
        rule.type(string(R.string.number), "8")
        rule.onNodeWithText(role(RoleEnum.OPPOSITE_HITTER)).performScrollTo().performClick()
        rule.save()
        rule.assertInvalid(string(R.string.number))
        assertEquals(listOf("Alessia Orro"), players().map { it.name })
    }

    @Test
    fun aNamesakeIsSavedAfterAsking() {
        core { it.savePlayer(team, null, PlayerForm("Paola Egonu", RoleEnum.OPPOSITE_HITTER, "18")) }
        show()
        rule.type(string(R.string.name), "paola egonu")
        rule.type(string(R.string.number), "81")
        rule.onNodeWithText(role(RoleEnum.OPPOSITE_HITTER)).performScrollTo().performClick()
        rule.save()
        rule.waitForText("Paola Egonu, number 18, is already in the team.")
        rule.onNodeWithText(string(R.string.cancel)).performClick()
        assertEquals(1, players().size)
        rule.save()
        rule.onNodeWithText(string(R.string.save_anyway)).performClick()
        rule.waitFor { done == 1 }
        assertEquals(listOf(18, 81), players().map { it.number.toInt() }.sorted())
    }

    /** A new player starts with the first number free in the team. */
    @Test
    fun aNewPlayerStartsWithTheFirstNumberFree() {
        core { it.savePlayer(team, null, PlayerForm("Alessia Orro", RoleEnum.SETTER, "1")) }
        core { it.savePlayer(team, null, PlayerForm("Monica De Gennaro", RoleEnum.LIBERO, "2")) }
        show()
        rule.field(string(R.string.number)).assertTextContains("3")
        rule.type(string(R.string.name), "Paola Egonu")
        rule.onNodeWithText(role(RoleEnum.OPPOSITE_HITTER)).performScrollTo().performClick()
        rule.save()
        rule.waitFor { done == 1 }
        assertEquals(listOf(1, 2, 3), players().map { it.number.toInt() }.sorted())
    }

    @Test
    fun aNewPlayer() {
        show()
        rule.type(string(R.string.name), "Paola Egonu")
        rule.type(string(R.string.number), "18")
        rule.onNodeWithText(role(RoleEnum.OPPOSITE_HITTER)).performScrollTo().performClick()
        rule.save()
        rule.waitFor { done == 1 }
        val player = players().single()
        assertEquals(Triple("Paola Egonu", 18, role(RoleEnum.OPPOSITE_HITTER)), Triple(player.name, player.number.toInt(), player.role))
    }

    @Test
    fun aPlayerEdited() {
        val id = core { it.savePlayer(team, null, PlayerForm("Paola Egonu", RoleEnum.OPPOSITE_HITTER, "18")) }
        show(id)
        rule.field(string(R.string.number)).assertTextContains("18")
        rule.type(string(R.string.number), "81")
        rule.save()
        rule.waitFor { done == 1 }
        assertEquals(listOf(81), players().map { it.number.toInt() })
    }

    @Test
    fun aPlayerRemovedAfterAsking() {
        val id = core { it.savePlayer(team, null, PlayerForm("Paola Egonu", RoleEnum.OPPOSITE_HITTER, "18")) }
        show(id)
        rule.onNodeWithContentDescription(string(R.string.remove_player)).performClick()
        rule.onNodeWithText(string(R.string.cancel)).performClick()
        assertEquals(1, players().size)
        rule.onNodeWithContentDescription(string(R.string.remove_player)).performClick()
        rule.onNodeWithText(string(R.string.remove_from_team)).performClick()
        rule.waitFor { done == 1 }
        assertEquals(0, players().size)
    }

    @Test
    fun aNewPlayerWithHeightAndNotes() {
        show()
        rule.type(string(R.string.name), "Paola Egonu")
        rule.type(string(R.string.number), "18")
        rule.onNodeWithText(role(RoleEnum.OPPOSITE_HITTER)).performScrollTo().performClick()
        rule.type(string(R.string.height), "193")
        rule.type(string(R.string.notes), "left-handed")
        rule.save()
        rule.waitFor { done == 1 }
        val player = players().single()
        assertEquals(Pair("193", "left-handed"), Pair(player.form.heightCm, player.form.notes))
    }

    @Test
    fun aHeightOutOfRangeIsRejected() {
        show()
        rule.type(string(R.string.name), "Paola Egonu")
        rule.type(string(R.string.number), "18")
        rule.onNodeWithText(role(RoleEnum.OPPOSITE_HITTER)).performScrollTo().performClick()
        rule.type(string(R.string.height), "1.93")
        rule.save()
        rule.assertInvalid(string(R.string.height))
        rule.assertValid(string(R.string.number))
        assertEquals(0, players().size)
    }

    @Test
    fun theBirthDateIsShownAndCanBeRemoved() {
        val id = core {
            it.savePlayer(
                team,
                null,
                PlayerForm("Paola Egonu", RoleEnum.OPPOSITE_HITTER, "18", birthDate = "1998-12-18"),
            )
        }
        show(id)
        val shown = LocalDate.of(1998, 12, 18).format(DateTimeFormatter.ofLocalizedDate(FormatStyle.LONG))
        // read only: a text, not a field to type in
        rule.onNodeWithText(shown).performScrollTo()
        rule.onNodeWithContentDescription(string(R.string.remove_birth_date)).performScrollTo().performClick()
        rule.save()
        rule.waitFor { done == 1 }
        assertEquals("", players().single().form.birthDate)
    }

    private fun fixture(name: String) = javaClass.classLoader!!.getResourceAsStream(name)!!.use { it.readBytes() }

    private fun modelOf(player: String): PlayerFormViewModel {
        val model = rule.runOnUiThread { PlayerFormViewModel(app, team, player) }
        rule.waitFor { model.state.value is PlayerFormState.Editing }
        return model
    }

    private fun editing(model: PlayerFormViewModel) = model.state.value as PlayerFormState.Editing

    @Test
    fun thePhotoOfAPlayerIsChosenStoredWithTheFormAndRemoved() {
        val player = core { it.savePlayer(team, null, it.newPlayerForm(team).copy(name = "Paola Egonu", role = RoleEnum.OPPOSITE_HITTER), false) }
        val model = modelOf(player)
        rule.runOnUiThread { model.pickPhoto(fixture("solvay_one_face.jpg")) }
        rule.waitFor { !editing(model).finding }
        assertEquals(null, editing(model).error)
        assertEquals(true, editing(model).changed)
        assertEquals(true, editing(model).shown != null)
        // stored with the save, not before
        assertEquals(null, core { it.personPhoto(player) })
        rule.runOnUiThread { model.save() }
        rule.waitFor { model.state.value == PlayerFormState.Done }
        assertEquals(true, core { it.personPhoto(player) } != null)
        // read again by the form, removed
        val again = modelOf(player)
        assertEquals(true, editing(again).shown != null)
        rule.runOnUiThread { again.removePhoto() }
        rule.runOnUiThread { again.save() }
        rule.waitFor { again.state.value == PlayerFormState.Done }
        assertEquals(null, core { it.personPhoto(player) })
    }

    @Test
    fun aNewPlayerGetsThePhotoChosenInItsForm() {
        val model = rule.runOnUiThread { PlayerFormViewModel(app, team, null) }
        rule.waitFor { model.state.value is PlayerFormState.Editing }
        rule.runOnUiThread { model.change(null) { it.copy(name = "Paola Egonu", role = RoleEnum.OPPOSITE_HITTER) } }
        rule.runOnUiThread { model.pickPhoto(fixture("solvay_one_face.jpg")) }
        rule.waitFor { !editing(model).finding }
        rule.runOnUiThread { model.save() }
        rule.waitFor { model.state.value == PlayerFormState.Done }
        assertEquals(true, core { it.personPhoto(players().single().id) } != null)
    }

    @Test
    fun thePhotoOfAPersonOfTheRegistryIsShownOnceChosenAndGoneWithAnotherName() {
        val player = core { it.savePlayer(team, null, it.newPlayerForm(team).copy(name = "Paola Egonu", role = RoleEnum.OPPOSITE_HITTER), false) }
        core { it.setPersonPhoto(player, it.cropFace(fixture("solvay_one_face.jpg"))) }
        val other = saveTeam("Altra")
        val model = rule.runOnUiThread { PlayerFormViewModel(app, other, null) }
        rule.waitFor { model.state.value is PlayerFormState.Editing }
        assertEquals(null, editing(model).shown)
        rule.runOnUiThread { model.pickExisting(editing(model).existing.single().label) }
        rule.waitFor { editing(model).shown != null }
        // a name written over is a new person: no photo
        rule.runOnUiThread { model.name("Paola Egonu bis") }
        assertEquals(null, editing(model).shown)
    }

    @Test
    fun aPhotoWithoutOrWithManyFacesIsAnErrorOfTheForm() {
        val player = core { it.savePlayer(team, null, it.newPlayerForm(team).copy(name = "Paola Egonu", role = RoleEnum.OPPOSITE_HITTER), false) }
        val model = modelOf(player)
        rule.runOnUiThread { model.pickPhoto(fixture("solvay_three_faces.jpg")) }
        rule.waitFor { !editing(model).finding }
        assertEquals(string(R.string.many_faces, 3), editing(model).error)
        rule.runOnUiThread { model.pickPhoto(ByteArray(0)) }
        rule.waitFor { !editing(model).finding }
        assertEquals(string(R.string.file_not_read), editing(model).error)
        assertEquals(false, editing(model).changed)
    }

    @Test
    fun thePhotoOfAPlayerIsShownInTheRowOfTheTeam() {
        val player = core { it.savePlayer(team, null, it.newPlayerForm(team).copy(name = "Paola Egonu", role = RoleEnum.OPPOSITE_HITTER), false) }
        core { it.setPersonPhoto(player, core { c -> c.cropFace(fixture("solvay_one_face.jpg")) }) }
        rule.setContent { Scout4AllTheme { PlayerPhoto(player) } }
        rule.waitFor { rule.onAllNodesWithTag(SHOWN_IMAGE).fetchSemanticsNodes().size == 1 }
    }

    @Test
    fun thePhotoIsLargeCentredAndItsButtonsAreRightBelow() {
        show()
        val panel = rule.onNodeWithTag(PHOTO_PANEL).getUnclippedBoundsInRoot()
        val circle = rule.onNodeWithTag(PHOTO_CIRCLE).getUnclippedBoundsInRoot()
        val choose = rule.onNodeWithText(string(R.string.choose_photo)).getUnclippedBoundsInRoot()
        val take = rule.onNodeWithText(string(R.string.take_photo)).getUnclippedBoundsInRoot()
        assertEquals(160.dp, circle.right - circle.left)
        assert(abs(((circle.left + circle.right) - (panel.left + panel.right)).value) < 1f) { "the photo is centred" }
        assert(choose.top >= circle.bottom && take.top >= circle.bottom) { "the buttons are below the photo" }
        assert(choose.top - circle.bottom < 32.dp) { "right below" }
    }
}
