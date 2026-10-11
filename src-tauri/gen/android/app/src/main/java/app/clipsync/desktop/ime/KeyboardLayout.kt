package app.clipsync.desktop.ime

/** Descrição pura do teclado (o quê e onde); a aparência fica no [KeyboardTheme]. */
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
    /** Backspace segurado por um tempo: passa a apagar palavra inteira. */
    object DeleteWord : KeyAction()
    /** Deslizar na barra de espaço: move o cursor [delta] caracteres. */
    data class MoveCursor(val delta: Int) : KeyAction()
}

data class Key(
    val action: KeyAction,
    /** Rótulo fixo; null = derivado do texto. */
    val label: String? = null,
    /** Largura relativa a uma tecla de letra. */
    val width: Float = 1f,
    /** Variantes no toque longo (acentos, símbolos alternativos). */
    val alternates: List<String> = emptyList(),
    /** Dica pequena no canto (ex.: o número da fileira de cima). */
    val hint: String? = null,
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
    /** Teclado numérico grande (campos de número e data). */
    const val NUMBER = "number"
    /** Teclado de telefone (`* # +`). */
    const val PHONE = "phone"
}

/** Layout QWERTY (pt-BR/en) com acentos no toque longo. */
object QwertyLayout {
    private fun t(ch: String, vararg alt: String) = Key(KeyAction.Text(ch), alternates = alt.toList())
    private fun s(ch: String, vararg alt: String) = Key(KeyAction.Text(ch, shifted = ch), alternates = alt.toList())
    /** Fileira de cima: o número aparece no canto e é o primeiro item do toque longo. */
    private fun n(ch: String, digit: String, vararg alt: String) =
        Key(KeyAction.Text(ch), alternates = listOf(digit) + alt.toList(), hint = digit)

    private val letters = KeyboardLayer(
        Layers.LETTERS,
        listOf(
            KeyRow(listOf(
                n("q", "1"), n("w", "2"), n("e", "3", "é", "ê", "è", "ë"), n("r", "4"), n("t", "5"),
                n("y", "6"), n("u", "7", "ú", "ü", "ù", "û"), n("i", "8", "í", "î", "ì", "ï"),
                n("o", "9", "ó", "õ", "ô", "ò", "ö"), n("p", "0"),
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

    /** Teclado numérico como o do Gboard: dígitos grandes em 3 colunas, função na quarta. */
    private fun pad(id: String, bottom: List<Key>) = KeyboardLayer(
        id,
        listOf(
            KeyRow(listOf(s("1"), s("2"), s("3"), s("-", "–", "/"))),
            KeyRow(listOf(s("4"), s("5"), s("6"), Key(KeyAction.Space, width = 1f))),
            KeyRow(listOf(s("7"), s("8"), s("9"), Key(KeyAction.Backspace, label = "⌫", width = 1f))),
            KeyRow(bottom),
        ),
    )

    private val number = pad(Layers.NUMBER, listOf(
        Key(KeyAction.Layer(Layers.SYMBOLS), label = "?123", width = 1f),
        s(",").copy(width = 0.5f),
        s("0"),
        s(".").copy(width = 0.5f),
        Key(KeyAction.Enter, label = "↵", width = 1f),
    ))

    private val phone = pad(Layers.PHONE, listOf(
        s("*").copy(width = 0.5f),
        s("#").copy(width = 0.5f),
        s("0", "+"),
        s("+"),
        Key(KeyAction.Enter, label = "↵", width = 1f),
    ))

    val layers: Map<String, KeyboardLayer> = listOf(letters, symbols, symbols2, number, phone).associateBy { it.id }
}
