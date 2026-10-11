package app.clipsync.desktop.ime

import android.content.Context
import android.graphics.Color
import android.graphics.Typeface
import android.os.Build
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
    /** Enter e troca de camada (?123/ABC) com cantos totalmente arredondados. */
    val pillKeys: Boolean get() = false
    fun palette(dark: Boolean): Palette
    /** Cores reais; temas podem usar as do sistema (Material You). */
    fun palette(context: Context, dark: Boolean): Palette = palette(dark)
    fun typeface(context: Context): Typeface? = null
}

/**
 * Tema padrão, no estilo do Gboard atual: sem sombra, Enter e ?123 em pílula e,
 * no Android 12+, cores do Material You (seguem o papel de parede).
 */
object DefaultTheme : KeyboardTheme {
    override val id = "default"
    override val label = "Default" // a UI traduz pelo `id` (ver KeyboardView.tsx)
    override val keyRadiusDp = 8f
    override val keyGapDp = 6f
    override val rowGapDp = 10f
    override val keyHeightDp = 46f
    override val labelSp = 23f
    override val functionLabelSp = 15f
    override val hasKeyShadow = false
    override val keyTextBold = false
    override val pillKeys = true

    private val light = Palette(
        background = 0xFFEEF0F7.toInt(),
        key = Color.WHITE,
        keyPressed = 0xFFDDE0EA.toInt(),
        keyText = 0xFF1B1B1F.toInt(),
        functionKey = 0xFFDCE2F9.toInt(),
        functionKeyPressed = 0xFFC3CCEC.toInt(),
        functionKeyText = 0xFF1B1B1F.toInt(),
        accentKey = 0xFFD3E0FF.toInt(),
        accentKeyPressed = 0xFFB4C8F8.toInt(),
        accentKeyText = 0xFF0E1D3E.toInt(),
        activeKey = 0xFF3F5F99.toInt(),
        activeKeyText = Color.WHITE,
        hint = 0xFF5D5F68.toInt(),
        popupBackground = Color.WHITE,
        popupText = 0xFF1B1B1F.toInt(),
        popupSelected = 0xFF3F5F99.toInt(),
        keyShadow = 0x22000000,
    )
    private val dark = Palette(
        background = 0xFF1B1C20.toInt(),
        key = 0xFF34353B.toInt(),
        keyPressed = 0xFF4A4B52.toInt(),
        keyText = 0xFFE4E2E6.toInt(),
        functionKey = 0xFF2C3243.toInt(),
        functionKeyPressed = 0xFF41495E.toInt(),
        functionKeyText = 0xFFE4E2E6.toInt(),
        accentKey = 0xFFAAC7FF.toInt(),
        accentKeyPressed = 0xFF8EB2F5.toInt(),
        accentKeyText = 0xFF0A305F.toInt(),
        activeKey = 0xFFAAC7FF.toInt(),
        activeKeyText = 0xFF0A305F.toInt(),
        hint = 0xFF9A9CA6.toInt(),
        popupBackground = 0xFF46474D.toInt(),
        popupText = 0xFFE4E2E6.toInt(),
        popupSelected = 0xFFAAC7FF.toInt(),
        keyShadow = 0x55000000,
    )

    override fun palette(dark: Boolean) = if (dark) this.dark else light

    override fun palette(context: Context, dark: Boolean): Palette {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) return palette(dark)
        fun sys(id: Int) = context.getColor(id)
        return runCatching {
            if (dark) Palette(
                background = sys(android.R.color.system_neutral1_900),
                key = sys(android.R.color.system_neutral2_800),
                keyPressed = sys(android.R.color.system_neutral2_700),
                keyText = sys(android.R.color.system_neutral1_50),
                functionKey = sys(android.R.color.system_accent2_800),
                functionKeyPressed = sys(android.R.color.system_accent2_700),
                functionKeyText = sys(android.R.color.system_neutral1_50),
                accentKey = sys(android.R.color.system_accent1_200),
                accentKeyPressed = sys(android.R.color.system_accent1_300),
                accentKeyText = sys(android.R.color.system_accent1_800),
                activeKey = sys(android.R.color.system_accent1_200),
                activeKeyText = sys(android.R.color.system_accent1_800),
                hint = sys(android.R.color.system_neutral2_400),
                popupBackground = sys(android.R.color.system_neutral2_700),
                popupText = sys(android.R.color.system_neutral1_50),
                popupSelected = sys(android.R.color.system_accent1_200),
                keyShadow = 0x55000000,
            ) else Palette(
                background = sys(android.R.color.system_neutral2_50),
                key = sys(android.R.color.system_neutral1_10),
                keyPressed = sys(android.R.color.system_neutral2_100),
                keyText = sys(android.R.color.system_neutral1_900),
                functionKey = sys(android.R.color.system_accent2_100),
                functionKeyPressed = sys(android.R.color.system_accent2_200),
                functionKeyText = sys(android.R.color.system_neutral1_900),
                accentKey = sys(android.R.color.system_accent1_100),
                accentKeyPressed = sys(android.R.color.system_accent1_200),
                accentKeyText = sys(android.R.color.system_accent1_900),
                activeKey = sys(android.R.color.system_accent1_600),
                activeKeyText = sys(android.R.color.system_accent1_0),
                hint = sys(android.R.color.system_neutral2_600),
                popupBackground = sys(android.R.color.system_neutral1_10),
                popupText = sys(android.R.color.system_neutral1_900),
                popupSelected = sys(android.R.color.system_accent1_600),
                keyShadow = 0x22000000,
            )
        }.getOrElse { palette(dark) }
    }

    /** Google Sans quando o aparelho tem (Pixel); senão cai na fonte do sistema. */
    override fun typeface(context: Context): Typeface? =
        Typeface.create("google-sans", Typeface.NORMAL).takeIf { it != Typeface.DEFAULT }
}

/** Tema ClipSync: tokens do design "Organic" (src/styles/tokens.css) e fonte Figtree. */
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
    override val pillKeys = true

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
