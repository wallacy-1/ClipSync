package app.clipsync.desktop.ime

/**
 * Descrição pura do teclado (o quê e onde); a aparência fica no [KeyboardTheme].
 */
sealed class KeyAction {
    /** Insere texto no campo. [shifted] é a versão com shift (null = uppercase do próprio). */
    data class Text(val text: String, val shifted: String? = null) : KeyAction()
    object Shift : KeyAction()
    object Backspace : KeyAction()
    object Enter : KeyAction()
    object Space : KeyAction()
    data class Layer(val id: String) : KeyAction()
    /** Troca para o próximo teclado do sistema (🌐). */
    object SwitchIme : KeyAction()
}

data class Key(
    val action: KeyAction,
    /** Rótulo fixo; null = derivado do texto. */
    val label: String? = null,
    /** Largura relativa a uma tecla de letra. */
    val width: Float = 1f,
    /** Variantes no toque longo (acentos, símbolos alternativos). */
    val alternates: List<String> = emptyList(),
) {
    val isFunction: Boolean get() = action !is KeyAction.Text && action !is KeyAction.Space
}

data class KeyRow(val keys: List<Key>) {
    val totalWidth: Float get() = keys.sumOf { it.width.toDouble() }.toFloat()
}

data class KeyboardLayer(val id: String, val rows: List<KeyRow>)

object Layers {
    const val LETTERS = "letters"
    const val SYMBOLS = "symbols"
    const val SYMBOLS_2 = "symbols2"
}

/** Layout QWERTY (pt-BR/en) com acentos no toque longo. */
object QwertyLayout {
    private fun t(ch: String, vararg alt: String) = Key(KeyAction.Text(ch), alternates = alt.toList())
    private fun s(ch: String, vararg alt: String) = Key(KeyAction.Text(ch, shifted = ch), alternates = alt.toList())

    private val letters = KeyboardLayer(
        Layers.LETTERS,
        listOf(
            KeyRow(listOf(
                t("q", "1"), t("w", "2"), t("e", "é", "ê", "è", "ë", "3"), t("r", "4"), t("t", "5"),
                t("y", "6"), t("u", "ú", "ü", "ù", "û", "7"), t("i", "í", "î", "ì", "ï", "8"),
                t("o", "ó", "õ", "ô", "ò", "ö", "9"), t("p", "0"),
            )),
            KeyRow(listOf(
                t("a", "á", "ã", "â", "à", "ä", "å"), t("s", "ß"), t("d"), t("f"), t("g"), t("h"), t("j"), t("k"), t("l"),
            )),
            KeyRow(listOf(
                Key(KeyAction.Shift, label = "⇧", width = 1.5f),
                t("z"), t("x"), t("c", "ç"), t("v"), t("b"), t("n", "ñ"), t("m"),
                Key(KeyAction.Backspace, label = "⌫", width = 1.5f),
            )),
            KeyRow(listOf(
                Key(KeyAction.Layer(Layers.SYMBOLS), label = "?123", width = 1.5f),
                Key(KeyAction.SwitchIme, label = "🌐", width = 1f),
                s(",", ";", "!", "¡"),
                Key(KeyAction.Space, width = 4f),
                s(".", ":", "?", "¿", "…"),
                Key(KeyAction.Enter, label = "↵", width = 1.5f),
            )),
        ),
    )

    private val symbols = KeyboardLayer(
        Layers.SYMBOLS,
        listOf(
            KeyRow(listOf(s("1"), s("2"), s("3"), s("4"), s("5"), s("6"), s("7"), s("8"), s("9"), s("0"))),
            KeyRow(listOf(s("@"), s("#"), s("$", "€", "£", "¥"), s("%"), s("&"), s("-", "–", "—"), s("+"), s("("), s(")"), s("/"))),
            KeyRow(listOf(
                Key(KeyAction.Layer(Layers.SYMBOLS_2), label = "=\\<", width = 1.5f),
                s("*"), s("\""), s("'"), s(":"), s(";"), s("!", "¡"), s("?", "¿"),
                Key(KeyAction.Backspace, label = "⌫", width = 1.5f),
            )),
            KeyRow(listOf(
                Key(KeyAction.Layer(Layers.LETTERS), label = "ABC", width = 1.5f),
                Key(KeyAction.SwitchIme, label = "🌐", width = 1f),
                s(","),
                Key(KeyAction.Space, width = 4f),
                s("."),
                Key(KeyAction.Enter, label = "↵", width = 1.5f),
            )),
        ),
    )

    private val symbols2 = KeyboardLayer(
        Layers.SYMBOLS_2,
        listOf(
            KeyRow(listOf(s("~"), s("`"), s("|"), s("•"), s("√"), s("π"), s("÷"), s("×"), s("¶"), s("∆"))),
            KeyRow(listOf(s("£"), s("¢"), s("€"), s("¥"), s("^"), s("°"), s("="), s("{"), s("}"), s("\\"))),
            KeyRow(listOf(
                Key(KeyAction.Layer(Layers.SYMBOLS), label = "?123", width = 1.5f),
                s("%"), s("©"), s("®"), s("™"), s("✓"), s("["), s("]"),
                Key(KeyAction.Backspace, label = "⌫", width = 1.5f),
            )),
            KeyRow(listOf(
                Key(KeyAction.Layer(Layers.LETTERS), label = "ABC", width = 1.5f),
                Key(KeyAction.SwitchIme, label = "🌐", width = 1f),
                s("<"),
                Key(KeyAction.Space, width = 4f),
                s(">"),
                Key(KeyAction.Enter, label = "↵", width = 1.5f),
            )),
        ),
    )

    val layers: Map<String, KeyboardLayer> = listOf(letters, symbols, symbols2).associateBy { it.id }
}
