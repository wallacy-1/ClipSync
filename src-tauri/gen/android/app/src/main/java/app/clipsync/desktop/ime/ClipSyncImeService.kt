package app.clipsync.desktop.ime

import android.content.Context
import android.content.SharedPreferences
import android.inputmethodservice.InputMethodService
import android.os.Build
import android.text.InputType
import android.text.TextUtils
import android.view.View
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager

/**
 * Teclado opcional do ClipSync. Nada do que é digitado é registrado, persistido ou
 * enviado; a única coisa observada fora do fluxo normal do teclado é o clipboard,
 * via [ClipboardBridge] (ver docs/ime.md).
 */
class ClipSyncImeService : InputMethodService() {

    private var keyboard: KeyboardView? = null
    private var layerId = Layers.LETTERS
    private var autoCap = false
    private var isPassword = false
    /** Shift ligado automaticamente (início de frase): desliga sozinho após uma tecla. */
    private var autoShift = false

    private val prefsListener = SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        if (key == ImePrefs.KEY_THEME) applyTheme()
    }

    override fun onCreate() {
        super.onCreate()
        ImePrefs.prefs(this).registerOnSharedPreferenceChangeListener(prefsListener)
        // Clipboard: único leitor do IME; entrega ao núcleo Rust ou ao inbox selado.
        ClipboardBridge.debug("service created")
        ClipboardBridge.start(this)
    }

    override fun onDestroy() {
        ClipboardBridge.stop()
        ClipboardBridge.debug("service destroyed")
        ImePrefs.prefs(this).unregisterOnSharedPreferenceChangeListener(prefsListener)
        super.onDestroy()
    }

    override fun onWindowShown() {
        super.onWindowShown()
        ClipboardBridge.imeShown = true
        // Colar logo em seguida tem que pegar o que o PC mandou enquanto o processo dormia.
        CoreBridge.wake()
    }

    override fun onWindowHidden() {
        super.onWindowHidden()
        ClipboardBridge.imeShown = false
    }

    override fun onCreateInputView(): View {
        val view = KeyboardView(this)
        view.listener = ::onKey
        keyboard = view
        applyTheme()
        return view
    }

    override fun onEvaluateFullscreenMode(): Boolean = false

    /**
     * O padrão do SDK esconde o teclado virtual quando há teclado físico conectado
     * (emulador com `hw.keyboard=yes`, tablets com capa). Mostramos mesmo assim,
     * como Gboard/SwiftKey; o usuário pode alternar pelo 🌐 ou pelo sistema.
     */
    override fun onEvaluateInputViewShown(): Boolean {
        super.onEvaluateInputViewShown()
        return true
    }

    private fun applyTheme() {
        keyboard?.theme = KeyboardThemes.byId(ImePrefs.themeId(this))
    }

    override fun onStartInputView(info: EditorInfo?, restarting: Boolean) {
        super.onStartInputView(info, restarting)
        val inputType = info?.inputType ?: InputType.TYPE_CLASS_TEXT
        val klass = inputType and InputType.TYPE_MASK_CLASS
        val variation = inputType and InputType.TYPE_MASK_VARIATION

        isPassword = klass == InputType.TYPE_CLASS_TEXT && (
            variation == InputType.TYPE_TEXT_VARIATION_PASSWORD ||
                variation == InputType.TYPE_TEXT_VARIATION_VISIBLE_PASSWORD ||
                variation == InputType.TYPE_TEXT_VARIATION_WEB_PASSWORD
            ) || (klass == InputType.TYPE_CLASS_NUMBER && variation == InputType.TYPE_NUMBER_VARIATION_PASSWORD)

        autoCap = !isPassword && klass == InputType.TYPE_CLASS_TEXT &&
            (inputType and InputType.TYPE_TEXT_FLAG_CAP_SENTENCES) != 0

        layerId = when (klass) {
            InputType.TYPE_CLASS_NUMBER, InputType.TYPE_CLASS_PHONE, InputType.TYPE_CLASS_DATETIME -> Layers.SYMBOLS
            else -> Layers.LETTERS
        }

        keyboard?.apply {
            layer = QwertyLayout.layers.getValue(layerId)
            enterLabel = enterLabelFor(info)
            shift = ShiftState.OFF
        }
        autoShift = false
        applyTheme()
        updateAutoShift()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        super.onFinishInputView(finishingInput)
        keyboard?.shift = ShiftState.OFF
        autoShift = false
    }

    override fun onUpdateSelection(
        oldSelStart: Int, oldSelEnd: Int, newSelStart: Int, newSelEnd: Int,
        candidatesStart: Int, candidatesEnd: Int,
    ) {
        super.onUpdateSelection(oldSelStart, oldSelEnd, newSelStart, newSelEnd, candidatesStart, candidatesEnd)
        updateAutoShift()
    }

    private fun enterLabelFor(info: EditorInfo?): String {
        if (info == null) return "↵"
        if ((info.imeOptions and EditorInfo.IME_FLAG_NO_ENTER_ACTION) != 0) return "↵"
        info.actionLabel?.let { if (!TextUtils.isEmpty(it)) return it.toString() }
        return when (info.imeOptions and EditorInfo.IME_MASK_ACTION) {
            EditorInfo.IME_ACTION_GO -> "Ir"
            EditorInfo.IME_ACTION_SEARCH -> "Buscar"
            EditorInfo.IME_ACTION_SEND -> "Enviar"
            EditorInfo.IME_ACTION_NEXT -> "Próximo"
            EditorInfo.IME_ACTION_DONE -> "OK"
            EditorInfo.IME_ACTION_PREVIOUS -> "Anterior"
            else -> "↵"
        }
    }

    /** Shift automático no início de frase, só quando o campo pede (CAP_SENTENCES). */
    private fun updateAutoShift() {
        val kb = keyboard ?: return
        if (!autoCap || kb.shift == ShiftState.LOCKED || layerId != Layers.LETTERS) return
        val ic = currentInputConnection ?: return
        val caps = ic.getCursorCapsMode(InputType.TYPE_TEXT_FLAG_CAP_SENTENCES) != 0
        if (caps && kb.shift == ShiftState.OFF) {
            kb.shift = ShiftState.ON
            autoShift = true
        } else if (!caps && autoShift && kb.shift == ShiftState.ON) {
            kb.shift = ShiftState.OFF
            autoShift = false
        }
    }

    private fun onKey(action: KeyAction) {
        val ic = currentInputConnection ?: return
        val kb = keyboard ?: return
        when (action) {
            is KeyAction.Text -> {
                val out = if (kb.shift != ShiftState.OFF) (action.shifted ?: action.text.uppercase()) else action.text
                ic.commitText(out, 1)
                if (kb.shift == ShiftState.ON) { kb.shift = ShiftState.OFF; autoShift = false }
            }
            KeyAction.Space -> {
                ic.commitText(" ", 1)
                if (kb.shift == ShiftState.ON && !autoShift) kb.shift = ShiftState.OFF
            }
            KeyAction.Backspace -> backspace()
            KeyAction.Enter -> enter()
            KeyAction.Shift -> { autoShift = false } // a view já alternou o estado
            is KeyAction.Layer -> {
                layerId = action.id
                kb.layer = QwertyLayout.layers.getValue(layerId)
                kb.shift = ShiftState.OFF
                autoShift = false
                updateAutoShift()
            }
            KeyAction.SwitchIme -> switchKeyboard()
        }
    }

    private fun backspace() {
        val ic = currentInputConnection ?: return
        val selected = ic.getSelectedText(0)
        if (!selected.isNullOrEmpty()) {
            ic.commitText("", 1)
            return
        }
        // Emoji/caracteres fora do BMP ocupam 2 unidades UTF-16.
        val before = ic.getTextBeforeCursor(2, 0)
        val n = if (before != null && before.length == 2 && Character.isSurrogatePair(before[0], before[1])) 2 else 1
        ic.deleteSurroundingText(n, 0)
    }

    private fun enter() {
        val ic = currentInputConnection ?: return
        val info = currentInputEditorInfo
        val options = info?.imeOptions ?: 0
        val multiline = ((info?.inputType ?: 0) and InputType.TYPE_TEXT_FLAG_MULTI_LINE) != 0
        val action = options and EditorInfo.IME_MASK_ACTION
        val noEnterAction = (options and EditorInfo.IME_FLAG_NO_ENTER_ACTION) != 0
        if (!noEnterAction && !multiline && action != EditorInfo.IME_ACTION_NONE && action != EditorInfo.IME_ACTION_UNSPECIFIED) {
            ic.performEditorAction(action)
        } else {
            ic.commitText("\n", 1)
        }
    }

    private fun switchKeyboard() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.P) {
            if (switchToNextInputMethod(false)) return
        }
        (getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager)?.showInputMethodPicker()
    }
}
