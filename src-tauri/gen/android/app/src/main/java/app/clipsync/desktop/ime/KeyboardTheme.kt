package app.clipsync.desktop.ime

import android.content.Context
import android.graphics.Color
import android.graphics.Typeface
import androidx.core.content.res.ResourcesCompat
import app.clipsync.desktop.R

data class Palette(
    val background: Int,
    val key: Int,
    val keyPressed: Int,
    val keyText: Int,
    val functionKey: Int,
    val functionKeyPressed: Int,
    val functionKeyText: Int,
    val accentKey: Int,
    val accentKeyPressed: Int,
    val accentKeyText: Int,
    /** Shift travado (caps lock) / camada ativa. */
    val activeKey: Int,
    val activeKeyText: Int,
    val hint: Int,
    val popupBackground: Int,
    val popupText: Int,
    val popupSelected: Int,
    val keyShadow: Int,
)

/**
 * Aparência do teclado. O [KeyboardView] desenha qualquer [KeyboardLayer]
 * com qualquer tema; adicionar um tema = implementar esta interface e
 * registrá-lo em [KeyboardThemes.all].
 */
interface KeyboardTheme {
    val id: String
    val label: String
    val keyRadiusDp: Float
    val keyGapDp: Float
    val rowGapDp: Float
    val keyHeightDp: Float
    val labelSp: Float
    val functionLabelSp: Float
    val hasKeyShadow: Boolean
    val keyTextBold: Boolean
    fun palette(dark: Boolean): Palette
    fun typeface(context: Context): Typeface? = null
}

/** Tema padrão: parece um teclado Android comum. Segue o modo claro/escuro do sistema. */
object DefaultTheme : KeyboardTheme {
    override val id = "default"
    override val label = "Padrão"
    override val keyRadiusDp = 6f
    override val keyGapDp = 5f
    override val rowGapDp = 9f
    override val keyHeightDp = 48f
    override val labelSp = 22f
    override val functionLabelSp = 15f
    override val hasKeyShadow = true
    override val keyTextBold = false

    private val light = Palette(
        background = 0xFFE8EAED.toInt(),
        key = Color.WHITE,
        keyPressed = 0xFFD5D8DC.toInt(),
        keyText = 0xFF1F1F1F.toInt(),
        functionKey = 0xFFC8CCD2.toInt(),
        functionKeyPressed = 0xFFB3B8BF.toInt(),
        functionKeyText = 0xFF1F1F1F.toInt(),
        accentKey = 0xFF3D6FE0.toInt(),
        accentKeyPressed = 0xFF2F5BC0.toInt(),
        accentKeyText = Color.WHITE,
        activeKey = 0xFF1F1F1F.toInt(),
        activeKeyText = Color.WHITE,
        hint = 0xFF6F7379.toInt(),
        popupBackground = Color.WHITE,
        popupText = 0xFF1F1F1F.toInt(),
        popupSelected = 0xFF3D6FE0.toInt(),
        keyShadow = 0x33000000,
    )
    private val dark = Palette(
        background = 0xFF1B1D20.toInt(),
        key = 0xFF33363B.toInt(),
        keyPressed = 0xFF4A4E55.toInt(),
        keyText = 0xFFF1F1F1.toInt(),
        functionKey = 0xFF25282C.toInt(),
        functionKeyPressed = 0xFF3A3E44.toInt(),
        functionKeyText = 0xFFF1F1F1.toInt(),
        accentKey = 0xFF5B8DEF.toInt(),
        accentKeyPressed = 0xFF4373D6.toInt(),
        accentKeyText = Color.WHITE,
        activeKey = 0xFFF1F1F1.toInt(),
        activeKeyText = 0xFF1B1D20.toInt(),
        hint = 0xFF9AA0A6.toInt(),
        popupBackground = 0xFF33363B.toInt(),
        popupText = 0xFFF1F1F1.toInt(),
        popupSelected = 0xFF5B8DEF.toInt(),
        keyShadow = 0x55000000,
    )

    override fun palette(dark: Boolean) = if (dark) this.dark else light
}

/**
 * Tema ClipSync: tokens do design "Organic" (src/styles/tokens.css) e fonte Figtree.
 */
object ClipSyncTheme : KeyboardTheme {
    override val id = "clipsync"
    override val label = "ClipSync"
    override val keyRadiusDp = 10f
    override val keyGapDp = 6f
    override val rowGapDp = 9f
    override val keyHeightDp = 48f
    override val labelSp = 21f
    override val functionLabelSp = 14f
    override val hasKeyShadow = true
    override val keyTextBold = true

    private val light = Palette(
        background = 0xFFEBDDC5.toInt(),      // surface
        key = 0xFFF9F4ED.toInt(),             // neutral-100
        keyPressed = 0xFFDCD3C4.toInt(),      // neutral-300
        keyText = 0xFF201E1D.toInt(),         // text
        functionKey = 0xFFDCD3C4.toInt(),     // neutral-300
        functionKeyPressed = 0xFFC0B6A5.toInt(), // neutral-400
        functionKeyText = 0xFF474238.toInt(), // neutral-800
        accentKey = 0xFFC67139.toInt(),       // accent
        accentKeyPressed = 0xFFB2622D.toInt(),// accent-600
        accentKeyText = 0xFFF5EAD8.toInt(),   // bg
        activeKey = 0xFF728157.toInt(),       // accent-2-600
        activeKeyText = 0xFFF0FAE1.toInt(),   // accent-2-100
        hint = 0xFF82796A.toInt(),            // neutral-600
        popupBackground = 0xFFF9F4ED.toInt(),
        popupText = 0xFF201E1D.toInt(),
        popupSelected = 0xFFC67139.toInt(),
        keyShadow = 0x242E2B25,
    )
    private val dark = Palette(
        background = 0xFF2E2B25.toInt(),      // neutral-900
        key = 0xFF474238.toInt(),             // neutral-800
        keyPressed = 0xFF645C50.toInt(),      // neutral-700
        keyText = 0xFFF9F4ED.toInt(),         // neutral-100
        functionKey = 0xFF3A362E.toInt(),
        functionKeyPressed = 0xFF564F44.toInt(),
        functionKeyText = 0xFFDCD3C4.toInt(), // neutral-300
        accentKey = 0xFFD67F48.toInt(),       // accent-500
        accentKeyPressed = 0xFFB2622D.toInt(),
        accentKeyText = 0xFF201E1D.toInt(),
        activeKey = 0xFF8FA073.toInt(),       // accent-2-500
        activeKeyText = 0xFF272E1B.toInt(),   // accent-2-900
        hint = 0xFFA19786.toInt(),            // neutral-500
        popupBackground = 0xFF474238.toInt(),
        popupText = 0xFFF9F4ED.toInt(),
        popupSelected = 0xFFD67F48.toInt(),
        keyShadow = 0x55000000,
    )

    override fun palette(dark: Boolean) = if (dark) this.dark else light

    @Volatile private var cached: Typeface? = null
    override fun typeface(context: Context): Typeface? {
        cached?.let { return it }
        val base = runCatching { ResourcesCompat.getFont(context, R.font.figtree) }.getOrNull()
        val tf = if (base != null && android.os.Build.VERSION.SDK_INT >= 28) Typeface.create(base, 600, false) else base
        cached = tf
        return tf
    }
}

object KeyboardThemes {
    val all: List<KeyboardTheme> = listOf(DefaultTheme, ClipSyncTheme)
    val default: KeyboardTheme get() = DefaultTheme
    fun byId(id: String?): KeyboardTheme = all.firstOrNull { it.id == id } ?: default
}
