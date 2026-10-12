package app.clipsync.desktop.ime

import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.content.SharedPreferences
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.net.Uri
import android.inputmethodservice.InputMethodService
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.text.InputType
import android.text.TextUtils
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.widget.FrameLayout
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.view.inputmethod.InputMethodSubtype
import androidx.core.content.FileProvider
import androidx.core.view.inputmethod.EditorInfoCompat
import androidx.core.view.inputmethod.InputConnectionCompat
import androidx.core.view.inputmethod.InputContentInfoCompat
import app.clipsync.desktop.R
import java.io.File
import java.io.InputStream
import java.util.concurrent.Executors
import java.util.Locale
import kotlin.math.abs

/**
 * Teclado opcional do ClipSync. Nada do que é digitado é registrado, persistido ou
 * enviado; a única coisa observada fora do fluxo normal do teclado é o clipboard,
 * via [ClipboardBridge] (ver docs/ime.md).
 */
class ClipSyncImeService : InputMethodService() {

    private var keyboard: KeyboardView? = null
    /** Fica por cima da área das teclas quando um painel (emoji, histórico) está aberto. */
    private var panelHost: FrameLayout? = null
    private var suggester: Suggester? = null
    /** Campo aceita sugestões (texto comum; nunca senha, URL ou e-mail). */
    private var suggestOn = false
    /** Campo aceita autocorreção (além de sugestões: não é nome, endereço, etc.). */
    private var autocorrectOn = false
    /** Troca recomendada para a palavra atual, aplicada no espaço: `palavra → correção`. */
    private var pendingCorrection: Pair<String, String>? = null
    /** Última autocorreção feita; um backspace logo depois desfaz (`original → corrigida`). */
    private var lastCorrection: Pair<String, String>? = null
    /** Palavra cuja autocorreção foi desfeita: o próximo espaço a mantém como está. */
    private var keptWord: String? = null
    private var layerId = Layers.LETTERS
    private var autoCap = false
    private var capsModes = 0
    private var isPassword = false
    /**
     * Campo da WebView do próprio ClipSync: roda na mesma thread do IME, então leituras síncronas
     * do campo só respondem no timeout de 2 s. Aqui o teclado só escreve.
     */
    private var ownField = false
    /** Shift ligado automaticamente (início de frase): desliga sozinho após uma tecla. */
    private var autoShift = false
    /** Para o "espaço duplo vira ponto". */
    private var lastSpaceAt = 0L
    /** `?123` aberto a partir das letras: espaço volta para as letras. */
    private var symbolsFromLetters = false
    /** Clip já colado pelo chip: não é oferecido de novo. Só em memória. */
    private var pastedClipId: String? = null
    private var clipboard: ClipboardManager? = null
    private val clipListener = ClipboardManager.OnPrimaryClipChangedListener { refreshClipChip() }
    /** Miniatura da imagem copiada, decodificada fora da thread principal. */
    private var thumbId: String? = null
    private var thumb: Bitmap? = null
    private val io = Executors.newSingleThreadExecutor { r -> Thread(r, "clipsync-ime-io") }
    private val main = Handler(Looper.getMainLooper())

    private val prefsListener = SharedPreferences.OnSharedPreferenceChangeListener { _, key ->
        if (key == ImePrefs.KEY_THEME) applyTheme()
    }

    override fun onCreate() {
        super.onCreate()
        ImePrefs.prefs(this).registerOnSharedPreferenceChangeListener(prefsListener)
        suggester = Suggester(this, ::onSuggestions)
        // Clipboard: único leitor do IME; entrega ao núcleo Rust ou ao inbox selado.
        ClipboardBridge.debug("service created")
        ClipboardBridge.start(this)
        clipboard = (getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager)?.also {
            it.addPrimaryClipChangedListener(clipListener)
        }
    }

    override fun onDestroy() {
        clipboard?.removePrimaryClipChangedListener(clipListener)
        io.shutdownNow()
        suggester?.close()
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
        view.onPasteClip = ::pasteClip
        view.onOpenApp = ::openApp
        view.onSuggestion = ::pickSuggestion
        view.onToolbarAction = { a ->
            when (a) {
                KeyboardView.ToolbarAction.EMOJI -> openEmoji()
                KeyboardView.ToolbarAction.HISTORY -> openHistory()
                KeyboardView.ToolbarAction.CLOSE_PANEL -> closePanel()
            }
        }
        keyboard = view
        val host = FrameLayout(this).apply { visibility = View.GONE }
        panelHost = host
        view.onKeysArea = { top, height ->
            host.layoutParams = FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, height).apply { topMargin = top }
        }
        applyTheme()
        return FrameLayout(this).apply {
            addView(view, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
            addView(host, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0))
        }
    }

    private fun showPanel(title: String, panel: View) {
        val host = panelHost ?: return
        host.removeAllViews()
        host.addView(panel, FrameLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
        host.visibility = View.VISIBLE
        keyboard?.panelTitle = title
    }

    private fun closePanel() {
        panelHost?.let { it.removeAllViews(); it.visibility = View.GONE }
        keyboard?.panelTitle = null
    }

    private fun openEmoji() {
        val kb = keyboard ?: return
        showPanel(
            getString(R.string.ime_panel_emoji),
            EmojiPanel(
                this, kb.colors, ImePrefs.recentEmojis(this),
                onEmoji = { e ->
                    currentInputConnection?.commitText(e, 1)
                    ImePrefs.addRecentEmoji(this, e)
                },
                onBackspace = ::backspace,
                onAbc = ::closePanel,
            ),
        )
    }

    private fun openHistory() {
        val kb = keyboard ?: return
        val title = getString(R.string.ime_panel_history)
        io.execute {
            val full = CoreBridge.history(HISTORY_LIMIT)?.let { HistoryItem.parse(it) }
            val items = full ?: inboxHistory()
            main.post {
                if (keyboard !== kb) return@post
                showPanel(title, HistoryPanel(this, kb.colors, kb.labelTypeface, items, full == null, ::pasteHistory, ::openApp))
            }
        }
    }

    /** Núcleo fora: o que o teclado guardou no inbox enquanto isso (texto e imagem), mais novo primeiro. */
    private fun inboxHistory(): List<HistoryItem> = runCatching {
        val out = ArrayList<Pair<Long, HistoryItem>>()
        val texts = org.json.JSONArray(ImeInbox.listJson(this))
        for (i in 0 until texts.length()) {
            val o = texts.getJSONObject(i)
            out += o.optLong("ts") to HistoryItem(o.optString("id"), "text", o.optString("text"), o.optBoolean("sensitive"), false, null, null)
        }
        val images = org.json.JSONArray(ImeInbox.listImagesJson(this))
        for (i in 0 until images.length()) {
            val o = images.getJSONObject(i)
            out += o.optLong("ts") to HistoryItem(o.optString("id"), "image", "", false, false, o.optString("path"), null)
        }
        out.sortedByDescending { it.first }.map { it.second }
    }.getOrDefault(emptyList())

    private fun pasteHistory(item: HistoryItem) {
        closePanel()
        if (!item.isImage) {
            currentInputConnection?.commitText(item.text, 1)
            return
        }
        val path = item.path ?: return
        pasteImage("image/png", clipId = null) { File(path).takeIf { it.isFile }?.inputStream() }
    }

    override fun onEvaluateFullscreenMode(): Boolean = false

    /**
     * A view tem uma faixa transparente no topo para o balão e as variações.
     * Ela não conta como teclado: o app não encolhe por causa dela e toques ali vão para o app.
     */
    override fun onComputeInsets(outInsets: InputMethodService.Insets) {
        super.onComputeInsets(outInsets)
        val kb = keyboard ?: return
        if (!isInputViewShown || !kb.isShown) return
        outInsets.contentTopInsets += kb.keyboardTop
        outInsets.visibleTopInsets += kb.keyboardTop
        outInsets.touchableInsets = InputMethodService.Insets.TOUCHABLE_INSETS_CONTENT
    }

    /** O SDK esconde o teclado virtual com teclado físico conectado; mostramos mesmo assim (troca pelo 🌐). */
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

        capsModes = inputType and (InputType.TYPE_TEXT_FLAG_CAP_CHARACTERS or
            InputType.TYPE_TEXT_FLAG_CAP_WORDS or InputType.TYPE_TEXT_FLAG_CAP_SENTENCES)
        ownField = info?.packageName == packageName
        // Sem checar a classe: há apps (Contatos) que mandam classe 0 com CAP_WORDS.
        autoCap = !isPassword && !ownField && capsModes != 0

        layerId = when (klass) {
            InputType.TYPE_CLASS_NUMBER, InputType.TYPE_CLASS_DATETIME -> Layers.NUMBER
            InputType.TYPE_CLASS_PHONE -> Layers.PHONE
            else -> Layers.LETTERS
        }

        keyboard?.apply {
            layer = QwertyLayout.layers.getValue(layerId)
            enterLabel = info?.takeIf { !noEnterAction(it) }?.actionLabel?.takeIf { !TextUtils.isEmpty(it) }?.toString()
            enterIcon = enterIconFor(info)
            shift = ShiftState.OFF
        }
        autoShift = false
        symbolsFromLetters = false
        lastSpaceAt = 0L
        val noSuggest = (inputType and InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS) != 0
        suggestOn = klass == InputType.TYPE_CLASS_TEXT && !isPassword && !ownField && !noSuggest && variation !in setOf(
            InputType.TYPE_TEXT_VARIATION_URI,
            InputType.TYPE_TEXT_VARIATION_EMAIL_ADDRESS,
            InputType.TYPE_TEXT_VARIATION_WEB_EMAIL_ADDRESS,
            InputType.TYPE_TEXT_VARIATION_FILTER,
        )
        autocorrectOn = suggestOn && variation in setOf(
            InputType.TYPE_TEXT_VARIATION_NORMAL,
            InputType.TYPE_TEXT_VARIATION_SHORT_MESSAGE,
            InputType.TYPE_TEXT_VARIATION_LONG_MESSAGE,
            InputType.TYPE_TEXT_VARIATION_EMAIL_SUBJECT,
            InputType.TYPE_TEXT_VARIATION_WEB_EDIT_TEXT,
        )
        lastCorrection = null
        keptWord = null
        closePanel()
        clearSuggestions()
        suggester?.setLocale(keyboardLocale())
        applyTheme()
        updateSpaceLabel()
        refreshClipChip()
        updateAutoShift()
    }

    override fun onFinishInputView(finishingInput: Boolean) {
        super.onFinishInputView(finishingInput)
        keyboard?.shift = ShiftState.OFF
        autoShift = false
        closePanel()
        clearSuggestions()
    }

    override fun onCurrentInputMethodSubtypeChanged(newSubtype: InputMethodSubtype?) {
        super.onCurrentInputMethodSubtypeChanged(newSubtype)
        updateSpaceLabel()
        suggester?.setLocale(keyboardLocale())
    }

    override fun onUpdateSelection(
        oldSelStart: Int, oldSelEnd: Int, newSelStart: Int, newSelEnd: Int,
        candidatesStart: Int, candidatesEnd: Int,
    ) {
        super.onUpdateSelection(oldSelStart, oldSelEnd, newSelStart, newSelEnd, candidatesStart, candidatesEnd)
        updateAutoShift()
        updateSuggestions()
    }

    private fun keyboardLocale(): Locale {
        val st = (getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager)?.currentInputMethodSubtype
        @Suppress("DEPRECATION")
        val tag = st?.languageTag?.takeIf { it.isNotEmpty() } ?: st?.locale?.replace('_', '-')
        return tag?.takeIf { it.isNotEmpty() }?.let { Locale.forLanguageTag(it) } ?: Locale.getDefault()
    }

    /** Palavra que termina no cursor (letras, apóstrofo e hífen internos); vazia no meio de palavra. */
    private fun currentWord(): String {
        val ic = currentInputConnection ?: return ""
        if (!ic.getSelectedText(0).isNullOrEmpty()) return ""
        val next = ic.getTextAfterCursor(1, 0)
        if (!next.isNullOrEmpty() && next[0].isLetter()) return ""
        val before = ic.getTextBeforeCursor(WORD_LOOKBACK, 0) ?: return ""
        var i = before.length
        while (i > 0 && (before[i - 1].isLetter() || (before[i - 1] in "'-" && i - 1 > 0 && before[i - 2].isLetter()))) i--
        return before.substring(i).trimStart('\'', '-')
    }

    private fun clearSuggestions() {
        suggester?.cancel()
        pendingCorrection = null
        keyboard?.apply { suggestions = emptyList(); suggestionPick = -1 }
    }

    private fun updateSuggestions() {
        if (!suggestOn || layerId != Layers.LETTERS || keyboard?.panelTitle != null) return clearSuggestions()
        val word = currentWord()
        if (word.isEmpty()) return clearSuggestions()
        suggester?.request(word)
    }

    /** No meio a escolha principal (correção em negrito ou a própria palavra); dos lados as alternativas. */
    private fun onSuggestions(r: Suggester.Result) {
        val kb = keyboard ?: return
        if (r.word != currentWord()) return
        val correction = r.correction?.takeIf { autocorrectOn && r.word != keptWord }
        pendingCorrection = correction?.let { r.word to it }
        val others = r.suggestions.filter { it != correction }
        val slots = if (correction != null) {
            listOf(r.word, correction) + others.take(1)
        } else {
            listOfNotNull(others.getOrNull(0), r.word, others.getOrNull(1))
        }
        kb.suggestions = if (slots.size == 1 && r.correct) emptyList() else slots
        kb.suggestionPick = if (correction != null) 1 else -1
    }

    private fun pickSuggestion(s: String) {
        val ic = currentInputConnection ?: return
        val word = currentWord()
        ic.beginBatchEdit()
        if (word.isNotEmpty()) ic.deleteSurroundingText(word.length, 0)
        ic.commitText("$s ", 1)
        ic.endBatchEdit()
        lastCorrection = null
        clearSuggestions()
    }

    private fun noEnterAction(info: EditorInfo) = (info.imeOptions and EditorInfo.IME_FLAG_NO_ENTER_ACTION) != 0

    private fun enterIconFor(info: EditorInfo?): EnterIcon {
        // Multilinha: o Enter quebra linha (ver [enter]), então o ícone também é de quebra de linha.
        if (info == null || noEnterAction(info) || (info.inputType and InputType.TYPE_TEXT_FLAG_MULTI_LINE) != 0) return EnterIcon.RETURN
        return when (info.imeOptions and EditorInfo.IME_MASK_ACTION) {
            EditorInfo.IME_ACTION_GO -> EnterIcon.GO
            EditorInfo.IME_ACTION_SEARCH -> EnterIcon.SEARCH
            EditorInfo.IME_ACTION_SEND -> EnterIcon.SEND
            EditorInfo.IME_ACTION_NEXT -> EnterIcon.NEXT
            EditorInfo.IME_ACTION_DONE -> EnterIcon.DONE
            EditorInfo.IME_ACTION_PREVIOUS -> EnterIcon.PREVIOUS
            else -> EnterIcon.RETURN
        }
    }

    private fun updateSpaceLabel() {
        val kb = keyboard ?: return
        val st = (getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager)?.currentInputMethodSubtype
        @Suppress("DEPRECATION")
        val tag = st?.languageTag?.takeIf { it.isNotEmpty() } ?: st?.locale?.replace('_', '-')
        kb.spaceLabel = tag?.takeIf { it.isNotEmpty() }?.let {
            val loc = Locale.forLanguageTag(it)
            loc.getDisplayLanguage(loc).replaceFirstChar { c -> c.titlecase(loc) }
        } ?: ""
    }

    /** Chip "colar" com o que foi copiado há pouco. O texto só é desenhado; nada é guardado. */
    private fun refreshClipChip() {
        val kb = keyboard ?: return
        val clip = currentClip()
        if (clip == null || clip.id == pastedClipId) { kb.clip = null; return }
        val uri = clip.imageUri
        if (uri == null) { kb.clip = ClipChip(clip.text, masked = clip.sensitive); return }
        val label = getString(R.string.ime_clip_image)
        if (thumbId == clip.id) { kb.clip = ClipChip(label, masked = false, image = thumb); return }
        kb.clip = ClipChip(label, masked = false)
        io.execute {
            val bmp = loadThumb(uri, (40 * resources.displayMetrics.density).toInt())
            main.post {
                thumb?.takeIf { it !== bmp }?.recycle()
                thumbId = clip.id
                thumb = bmp
                if (currentClip()?.id == clip.id) refreshClipChip()
            }
        }
    }

    private fun loadThumb(uri: Uri, max: Int): Bitmap? = runCatching {
        val bounds = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        contentResolver.openInputStream(uri)?.use { BitmapFactory.decodeStream(it, null, bounds) }
        var sample = 1
        while (minOf(bounds.outWidth, bounds.outHeight) / (sample * 2) >= max) sample *= 2
        contentResolver.openInputStream(uri)?.use {
            BitmapFactory.decodeStream(it, null, BitmapFactory.Options().apply { inSampleSize = sample })
        }
    }.onFailure { ClipboardBridge.debug("thumb failed: $it") }.getOrNull()

    /** O que está no clipboard: texto, ou imagem (aí [imageUri] aponta para ela e [text] fica vazio). */
    private class CurrentClip(
        val id: String,
        val text: String,
        val sensitive: Boolean,
        val imageUri: Uri? = null,
        val imageMime: String? = null,
    )

    private fun currentClip(): CurrentClip? {
        val clip = runCatching { clipboard?.primaryClip }.getOrNull() ?: return null
        if (clip.itemCount == 0) return null
        val desc = clip.description
        val ts = desc.timestamp
        if (ts > 0 && clipAgeMs(ts) > CLIP_CHIP_MAX_AGE_MS) return null
        val sensitive = desc.extras?.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE, false) ?: false
        val item = clip.getItemAt(0)
        if (desc.hasMimeType("image/*")) {
            val uri = item.uri ?: return null
            val mime = (0 until desc.mimeTypeCount).map { desc.getMimeType(it) }.firstOrNull { it.startsWith("image/") }
            return CurrentClip("$ts:$uri", "", sensitive, uri, mime ?: "image/png")
        }
        val isText = desc.hasMimeType(ClipDescription.MIMETYPE_TEXT_PLAIN) || desc.hasMimeType(ClipDescription.MIMETYPE_TEXT_HTML)
        if (!isText) return null
        val text = item.coerceToText(this)?.toString() ?: return null
        if (text.isBlank()) return null
        return CurrentClip("$ts:${text.hashCode()}", text, sensitive)
    }

    /** O horário do clip vem em `currentTimeMillis`; versões antigas usaram tempo desde o boot. */
    private fun clipAgeMs(ts: Long): Long =
        minOf(abs(System.currentTimeMillis() - ts), abs(SystemClock.elapsedRealtime() - ts))

    private fun pasteClip() {
        val ic = currentInputConnection ?: return
        val clip = currentClip() ?: return refreshClipChip()
        val uri = clip.imageUri
        if (uri != null) return pasteImage(clip.imageMime ?: "image/png", clip.id) { contentResolver.openInputStream(uri) }
        ic.commitText(clip.text, 1)
        pastedClipId = clip.id
        refreshClipChip()
    }

    /**
     * Cola imagem pelo `commitContent` (como o Gboard), só em campos que declaram aceitar
     * imagem. A cópia vai para o cache do app e é servida pelo FileProvider, porque a
     * permissão de leitura que o clipboard deu ao teclado não passa para o outro app.
     */
    private fun pasteImage(mime: String, clipId: String?, open: () -> InputStream?) {
        val info = currentInputEditorInfo ?: return
        val accepts = EditorInfoCompat.getContentMimeTypes(info).any { ClipDescription.compareMimeTypes(mime, it) }
        if (!accepts) {
            keyboard?.showNotice(getString(R.string.ime_clip_image_unsupported))
            return
        }
        io.execute {
            val uri = runCatching {
                val dir = File(cacheDir, PASTE_DIR).apply { mkdirs() }
                dir.listFiles()?.sortedByDescending { it.lastModified() }?.drop(PASTE_KEEP - 1)?.forEach { it.delete() }
                val ext = mime.substringAfter('/').substringBefore('+').ifEmpty { "png" }
                val target = File(dir, "paste-${System.currentTimeMillis()}.$ext")
                val copied = open()?.use { input -> target.outputStream().use { input.copyTo(it) } }
                if (copied == null) null else FileProvider.getUriForFile(this, "$packageName.fileprovider", target)
            }.getOrNull()
            main.post {
                val ic = currentInputConnection ?: return@post
                val ed = currentInputEditorInfo ?: return@post
                val content = uri?.let { InputContentInfoCompat(it, ClipDescription(getString(R.string.ime_clip_image), arrayOf(mime)), null) }
                val ok = content != null &&
                    InputConnectionCompat.commitContent(ic, ed, content, InputConnectionCompat.INPUT_CONTENT_GRANT_READ_URI_PERMISSION, null)
                if (ok) { if (clipId != null) pastedClipId = clipId; refreshClipChip() }
                else keyboard?.showNotice(getString(R.string.ime_clip_image_unsupported))
            }
        }
    }

    private fun openApp() {
        val intent = packageManager.getLaunchIntentForPackage(packageName) ?: return
        intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        requestHideSelf(0)
        runCatching { startActivity(intent) }
    }

    private fun updateAutoShift() {
        val kb = keyboard ?: return
        if (!autoCap || kb.shift == ShiftState.LOCKED || layerId != Layers.LETTERS) return
        val ic = currentInputConnection ?: return
        val caps = ic.getCursorCapsMode(capsModes) != 0
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
        if (action != KeyAction.Space) lastSpaceAt = 0L
        if (action != KeyAction.Backspace && action != KeyAction.Shift) lastCorrection = null
        when (action) {
            is KeyAction.Text -> {
                val out = if (kb.shift != ShiftState.OFF) (action.shifted ?: action.text.uppercase()) else action.text
                ic.commitText(out, 1)
                if (kb.shift == ShiftState.ON) { kb.shift = ShiftState.OFF; autoShift = false }
            }
            KeyAction.Space -> space()
            KeyAction.Backspace -> backspace()
            KeyAction.DeleteWord -> deleteWord()
            is KeyAction.MoveCursor -> moveCursor(action.delta)
            KeyAction.Enter -> enter()
            KeyAction.Shift -> { autoShift = false } // a view já alternou o estado
            is KeyAction.Layer -> {
                symbolsFromLetters = when {
                    action.id == Layers.LETTERS -> false
                    layerId == Layers.LETTERS -> true
                    else -> symbolsFromLetters
                }
                layerId = action.id
                kb.layer = QwertyLayout.layers.getValue(layerId)
                kb.shift = ShiftState.OFF
                autoShift = false
                updateAutoShift()
            }
            KeyAction.SwitchIme -> switchKeyboard()
        }
    }

    private fun space() {
        val ic = currentInputConnection ?: return
        val kb = keyboard ?: return
        val now = SystemClock.uptimeMillis()
        // Espaço duplo depois de uma palavra vira ". ". Nunca em campo de senha.
        if (!isPassword && !ownField && layerId == Layers.LETTERS && now - lastSpaceAt < DOUBLE_SPACE_MS) {
            val before = ic.getTextBeforeCursor(2, 0)
            if (before != null && before.length == 2 && before[1] == ' ' && before[0].isLetterOrDigit()) {
                ic.beginBatchEdit()
                ic.deleteSurroundingText(1, 0)
                ic.commitText(". ", 1)
                ic.endBatchEdit()
                lastSpaceAt = 0L
                return
            }
        }
        val correction = pendingCorrection
        if (correction != null && currentWord() == correction.first) {
            // Autocorreção: troca a palavra junto com o espaço; backspace em seguida desfaz.
            ic.beginBatchEdit()
            ic.deleteSurroundingText(correction.first.length, 0)
            ic.commitText(correction.second + " ", 1)
            ic.endBatchEdit()
            lastCorrection = correction
            clearSuggestions()
        } else {
            ic.commitText(" ", 1)
        }
        lastSpaceAt = now
        if (kb.shift == ShiftState.ON && !autoShift) kb.shift = ShiftState.OFF
        if (layerId != Layers.LETTERS && symbolsFromLetters) onKey(KeyAction.Layer(Layers.LETTERS))
    }

    private fun backspace() {
        val ic = currentInputConnection ?: return
        // A tecla apaga a seleção e pares substitutos sem ler o campo.
        if (ownField) return sendDownUpKeyEvents(KeyEvent.KEYCODE_DEL)
        val undo = lastCorrection
        lastCorrection = null
        if (undo != null && ic.getTextBeforeCursor(undo.second.length + 1, 0)?.toString() == undo.second + " ") {
            ic.beginBatchEdit()
            ic.deleteSurroundingText(undo.second.length + 1, 0)
            ic.commitText(undo.first, 1)
            ic.endBatchEdit()
            keptWord = undo.first
            return
        }
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

    private fun deleteWord() {
        val ic = currentInputConnection ?: return
        if (ownField) return backspace()
        if (!ic.getSelectedText(0).isNullOrEmpty()) return backspace()
        val before = ic.getTextBeforeCursor(64, 0) ?: return backspace()
        var i = before.length
        while (i > 0 && before[i - 1].isWhitespace()) i--
        while (i > 0 && !before[i - 1].isWhitespace()) i--
        val n = before.length - i
        if (n > 0) ic.deleteSurroundingText(n, 0)
    }

    private fun moveCursor(delta: Int) {
        val code = if (delta < 0) KeyEvent.KEYCODE_DPAD_LEFT else KeyEvent.KEYCODE_DPAD_RIGHT
        repeat(abs(delta)) { sendDownUpKeyEvents(code) }
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

    private companion object {
        const val DOUBLE_SPACE_MS = 1_200L
        const val CLIP_CHIP_MAX_AGE_MS = 10 * 60_000L
        const val PASTE_DIR = "ime-paste"
        const val HISTORY_LIMIT = 30
        const val WORD_LOOKBACK = 48
        const val PASTE_KEEP = 3
    }
}
