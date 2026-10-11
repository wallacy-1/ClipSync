package app.clipsync.desktop.ime

import android.content.Context
import android.content.res.Configuration
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Rect
import android.graphics.Paint
import android.graphics.Path
import android.graphics.RectF
import android.graphics.Typeface
import android.graphics.drawable.Drawable
import android.os.Handler
import android.os.Build
import android.os.Looper
import android.text.TextPaint
import android.text.TextUtils
import android.util.TypedValue
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.WindowInsets
import androidx.core.content.ContextCompat
import app.clipsync.desktop.R
import kotlin.math.abs
import kotlin.math.max

enum class ShiftState { OFF, ON, LOCKED }

enum class EnterIcon { RETURN, GO, SEARCH, SEND, NEXT, DONE, PREVIOUS }

/** O que está no clipboard, oferecido na barra de cima para colar com um toque. */
data class ClipChip(val text: String, val masked: Boolean, val image: Bitmap? = null)

/**
 * Renderiza uma [KeyboardLayer] com um [KeyboardTheme] e traduz toques em [KeyAction].
 * View própria porque o `KeyboardView` do SDK é deprecated (API 29).
 */
class KeyboardView(context: Context) : View(context) {

    var listener: ((KeyAction) -> Unit)? = null
    var onPasteClip: (() -> Unit)? = null
    var onOpenApp: (() -> Unit)? = null

    var theme: KeyboardTheme = KeyboardThemes.default
        set(value) { field = value; applyTheme(); requestLayout(); invalidate() }

    var layer: KeyboardLayer = QwertyLayout.layers.getValue(Layers.LETTERS)
        set(value) { field = value; requestLayout(); invalidate() }

    var shift: ShiftState = ShiftState.OFF
        set(value) { field = value; invalidate() }

    var enterIcon: EnterIcon = EnterIcon.RETURN
        set(value) { field = value; invalidate() }

    /** Rótulo próprio do campo (`EditorInfo.actionLabel`); quando existe, vence o ícone. */
    var enterLabel: String? = null
        set(value) { field = value; invalidate() }

    /** Idioma atual, escrito discretamente na barra de espaço. */
    var spaceLabel: String = ""
        set(value) { field = value; invalidate() }

    var clip: ClipChip? = null
        set(value) { field = value; invalidate() }

    /** Botões da barra de cima que o service trata (emoji, histórico, fechar painel). */
    enum class ToolbarAction { EMOJI, HISTORY, CLOSE_PANEL }
    var onToolbarAction: ((ToolbarAction) -> Unit)? = null

    /** Sugestões da palavra em digitação, na ordem em que aparecem (esquerda, meio, direita). */
    var suggestions: List<String> = emptyList()
        set(value) { field = value; invalidate() }
    /** Sugestão que o espaço vai aplicar (autocorreção), em negrito; -1 = nenhuma. */
    var suggestionPick: Int = -1
        set(value) { field = value; invalidate() }
    var onSuggestion: ((String) -> Unit)? = null

    /**
     * Painel aberto (emoji, histórico) com este título: as teclas somem, o painel (outra view,
     * por cima da área das teclas) assume, e a barra mostra voltar + título.
     */
    var panelTitle: String? = null
        set(value) { field = value; pressed = null; popupSlot = null; invalidate() }

    /** Área das teclas, em px a partir do topo desta view, para posicionar os painéis. */
    var onKeysArea: ((top: Int, height: Int) -> Unit)? = null
    private var lastKeysArea = -1 to -1

    /** Cores atuais, para os painéis seguirem o mesmo tema. */
    val colors: Palette get() = palette
    val labelTypeface: Typeface get() = typeface ?: Typeface.DEFAULT

    /**
     * Aviso curto na barra de cima, no lugar do chip. Toast não serve: o Android
     * o suprime quando o app não tem permissão de notificação.
     */
    private var notice: String? = null
    private val clearNotice = Runnable { notice = null; invalidate() }

    fun showNotice(text: String) {
        notice = text
        removeCallbacks(clearNotice)
        postDelayed(clearNotice, 2_500L)
        invalidate()
    }

    private class Slot(val key: Key, val rect: RectF)
    private val slots = ArrayList<Slot>()
    private var palette: Palette = theme.palette(false)
    private var typeface: Typeface? = null
    private val dm get() = resources.displayMetrics
    private fun dp(v: Float) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, dm)
    private fun sp(v: Float) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, v, dm)
    private val sidePadding get() = dp(3f)
    /**
     * Faixa transparente acima do teclado onde o balão e as variações podem
     * aparecer por cima do app (como no Gboard). Fora do teclado para toques:
     * o service exclui a faixa em `onComputeInsets`.
     */
    private val overlayHeight get() = dp(64f)
    /** Onde o teclado de fato começa, em px a partir do topo da view. */
    val keyboardTop: Int get() = overlayHeight.toInt()
    /** Em paisagem teclas e barra encolhem para sobrar espaço ao app. */
    private val landscape get() = resources.configuration.orientation == Configuration.ORIENTATION_LANDSCAPE
    private val keyHeightPx get() = dp(theme.keyHeightDp * if (landscape) 0.72f else 1f)
    private val rowGapPx get() = dp(theme.rowGapDp * if (landscape) 0.6f else 1f)
    private val toolbarHeight get() = dp(if (landscape) 38f else 44f)
    private val topPadding get() = dp(4f)
    /** Barra de navegação/gestos por baixo do teclado (edge-to-edge): vira padding inferior. */
    private var navBottom = 0
    private val bottomPadding get() = dp(6f) + navBottom

    private val keyPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val shadowPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val labelPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { textAlign = Paint.Align.CENTER }
    private val hintPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { textAlign = Paint.Align.RIGHT }
    private val popupPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val iconPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
    }
    private val chipTextPaint = TextPaint(Paint.ANTI_ALIAS_FLAG)
    private val thumbPaint = Paint(Paint.ANTI_ALIAS_FLAG or Paint.FILTER_BITMAP_FLAG)

    private val handler = Handler(Looper.getMainLooper())
    private var activePointer = MotionEvent.INVALID_POINTER_ID
    private var pressed: Slot? = null
    private var longPressFired = false
    private var popupSlot: Slot? = null
    private var popupItems: List<String> = emptyList()
    private var popupSelected = -1
    private val popupRect = RectF()
    /** Tecla cujo balão continua visível um instante depois de soltar. */
    private var lingerSlot: Slot? = null
    private var lingerLabel = ""
    private val hideLinger = Runnable { lingerSlot = null; invalidate() }
    private val lingerMs = 80L
    private val bgPaint = Paint()
    private val longPressMs = ViewConfiguration.getLongPressTimeout().toLong()
    private val repeatStartMs = 350L
    private val repeatIntervalMs = 45L
    /** Depois de tantas repetições o backspace passa a apagar palavras. */
    private val repeatsBeforeWords = 14
    private val wordRepeatIntervalMs = 160L
    private var repeatCount = 0
    private var lastShiftTapAt = 0L

    /** Deslizar na barra de espaço move o cursor em vez de inserir espaço. */
    private var spaceStartX = 0f
    private var cursorLastX = 0f
    private var cursorMode = false

    private enum class ToolbarTarget { CLIP, APP, EMOJI, HISTORY, BACK, SUGGESTION }
    private var toolbarPressed: ToolbarTarget? = null
    private var pressedSuggestion = -1
    private val chipRect = RectF()
    private val appRect = RectF()
    private val emojiRect = RectF()
    private val historyRect = RectF()
    private val backRect = RectF()
    private val suggestionRects = List(3) { RectF() }
    private var appIcon: Drawable? = null

    private val longPressRunnable = Runnable { onLongPress() }
    private val repeatRunnable = object : Runnable {
        override fun run() {
            val s = pressed ?: return
            if (s.key.action !is KeyAction.Backspace) return
            repeatCount++
            val words = repeatCount > repeatsBeforeWords
            listener?.invoke(if (words) KeyAction.DeleteWord else KeyAction.Backspace)
            handler.postDelayed(this, if (words) wordRepeatIntervalMs else repeatIntervalMs)
        }
    }

    init {
        isClickable = true
        isHapticFeedbackEnabled = true
        appIcon = ContextCompat.getDrawable(context, R.drawable.ic_stat_clipsync)?.mutate()
        applyTheme()
    }

    val isDark: Boolean
        get() = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

    private fun applyTheme() {
        palette = theme.palette(context, isDark)
        typeface = theme.typeface(context)
        labelPaint.typeface = typeface ?: Typeface.DEFAULT
        hintPaint.typeface = typeface ?: Typeface.DEFAULT
        chipTextPaint.typeface = typeface ?: Typeface.DEFAULT
        bgPaint.color = palette.background
    }

    override fun onApplyWindowInsets(insets: WindowInsets): WindowInsets {
        val bottom = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            insets.getInsets(WindowInsets.Type.navigationBars() or WindowInsets.Type.mandatorySystemGestures()).bottom
        } else {
            @Suppress("DEPRECATION") insets.systemWindowInsetBottom
        }
        if (bottom != navBottom) { navBottom = bottom; requestLayout() }
        return super.onApplyWindowInsets(insets)
    }

    override fun onConfigurationChanged(newConfig: Configuration?) {
        super.onConfigurationChanged(newConfig)
        applyTheme()
        requestLayout()
    }

    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        val w = MeasureSpec.getSize(widthMeasureSpec)
        val rows = layer.rows.size
        val h = overlayHeight + toolbarHeight + topPadding + bottomPadding + rows * keyHeightPx + max(0, rows - 1) * rowGapPx
        setMeasuredDimension(w, h.toInt())
    }

    override fun onSizeChanged(w: Int, h: Int, oldw: Int, oldh: Int) {
        super.onSizeChanged(w, h, oldw, oldh)
        layoutKeys()
    }

    override fun requestLayout() {
        super.requestLayout()
        if (width > 0) layoutKeys()
    }

    private fun layoutKeys() {
        slots.clear()
        val gap = dp(theme.keyGapDp)
        val rowGap = rowGapPx
        val keyH = keyHeightPx
        val usable = width - 2 * sidePadding
        // Largura de uma unidade: a linha mais larga define (normalmente 10 teclas).
        val maxUnits = layer.rows.maxOf { it.totalWidth }
        val unit = (usable - (maxOf(layer.rows.maxOf { it.keys.size }, 1) - 1) * gap) / maxUnits
        var y = overlayHeight + toolbarHeight + topPadding
        for (row in layer.rows) {
            val rowW = row.totalWidth * unit + (row.keys.size - 1) * gap
            var x = sidePadding + (usable - rowW) / 2f
            for (key in row.keys) {
                val w = key.width * unit
                slots.add(Slot(key, RectF(x, y, x + w, y + keyH)))
                x += w + gap
            }
            y += keyH + rowGap
        }
        val top = (overlayHeight + toolbarHeight).toInt()
        val area = top to (y - rowGap + dp(2f)).toInt() - top
        if (area != lastKeysArea) {
            lastKeysArea = area
            onKeysArea?.invoke(area.first, area.second)
        }
    }

    // ---- desenho ----

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val radius = dp(theme.keyRadiusDp)
        canvas.drawRect(0f, overlayHeight, width.toFloat(), height.toFloat(), bgPaint)
        drawToolbar(canvas)
        if (panelTitle != null) return
        val shadowDy = dp(1f)
        for (slot in slots) {
            val key = slot.key
            val isPressed = slot === pressed && !cursorMode
            val (bg, fg) = colorsFor(key, isPressed)
            val pill = theme.pillKeys && (key.action is KeyAction.Enter || key.action is KeyAction.Layer)
            val kr = if (pill) slot.rect.height() / 2f else radius
            if (theme.hasKeyShadow) {
                shadowPaint.color = palette.keyShadow
                val r = RectF(slot.rect); r.offset(0f, shadowDy)
                canvas.drawRoundRect(r, kr, kr, shadowPaint)
            }
            keyPaint.color = bg
            canvas.drawRoundRect(slot.rect, kr, kr, keyPaint)

            if (drawKeyIcon(canvas, key, slot.rect, fg)) continue

            if (key.action is KeyAction.Space) {
                drawSpaceLabel(canvas, slot.rect)
                continue
            }

            val label = labelFor(key)
            labelPaint.color = fg
            labelPaint.textSize = sp(if (key.isFunction || label.length > 1) theme.functionLabelSp else theme.labelSp)
            labelPaint.isFakeBoldText = theme.keyTextBold && !key.isFunction && typeface == null
            // Rótulos longos ("Próximo", "Buscar") encolhem para caber na tecla.
            val maxW = slot.rect.width() - dp(8f)
            while (labelPaint.measureText(label) > maxW && labelPaint.textSize > sp(9f)) labelPaint.textSize -= sp(1f)
            val fm = labelPaint.fontMetrics
            val baseline = slot.rect.centerY() - (fm.ascent + fm.descent) / 2f
            canvas.drawText(label, slot.rect.centerX(), baseline, labelPaint)

            key.hint?.let {
                hintPaint.color = palette.hint
                hintPaint.textSize = sp(10.5f)
                canvas.drawText(it, slot.rect.right - dp(5f), slot.rect.top + dp(13f), hintPaint)
            }
        }
        if (popupSlot != null) drawPopup(canvas, radius)
        else {
            val p = pressed
            if (p != null && !cursorMode && p.key.action is KeyAction.Text) drawPreview(canvas, p, labelFor(p.key), radius)
            else lingerSlot?.let { drawPreview(canvas, it, lingerLabel, radius) }
        }
    }

    private fun colorsFor(key: Key, isPressed: Boolean): Pair<Int, Int> = when (key.action) {
        is KeyAction.Enter -> (if (isPressed) palette.accentKeyPressed else palette.accentKey) to palette.accentKeyText
        is KeyAction.Shift -> when {
            shift == ShiftState.LOCKED -> palette.activeKey to palette.activeKeyText
            isPressed -> palette.functionKeyPressed to palette.functionKeyText
            else -> palette.functionKey to palette.functionKeyText
        }
        is KeyAction.Text, is KeyAction.Space ->
            (if (isPressed) palette.keyPressed else palette.key) to palette.keyText
        else -> (if (isPressed) palette.functionKeyPressed else palette.functionKey) to palette.functionKeyText
    }

    private fun labelFor(key: Key): String {
        key.label?.let { if (key.action !is KeyAction.Enter) return it }
        return when (val a = key.action) {
            is KeyAction.Text -> if (shift != ShiftState.OFF) (a.shifted ?: a.text.uppercase()) else a.text
            is KeyAction.Enter -> enterLabel ?: ""
            is KeyAction.Space -> ""
            else -> key.label ?: ""
        }
    }

    private fun drawSpaceLabel(canvas: Canvas, rect: RectF) {
        // No teclado numérico o espaço é uma tecla comum: o nome do idioma não cabe nem ajuda.
        if (spaceLabel.isEmpty() || rect.width() < rect.height() * 2.5f) return
        labelPaint.color = palette.hint
        labelPaint.textSize = sp(12.5f)
        labelPaint.isFakeBoldText = false
        val fm = labelPaint.fontMetrics
        canvas.drawText(spaceLabel, rect.centerX(), rect.centerY() - (fm.ascent + fm.descent) / 2f, labelPaint)
    }

    /** Teclas de função com ícone vetorial (em vez de ⇧ ⌫ 🌐 ↵, que variam de fonte para fonte). */
    private fun drawKeyIcon(canvas: Canvas, key: Key, rect: RectF, color: Int): Boolean {
        val size = dp(22f)
        when (key.action) {
            is KeyAction.Shift -> {
                val filled = shift != ShiftState.OFF
                drawIcon(canvas, Icons.shift, rect.centerX(), rect.centerY(), size, color, filled)
                if (shift == ShiftState.LOCKED) drawIcon(canvas, Icons.capsBar, rect.centerX(), rect.centerY(), size, color)
            }
            is KeyAction.Backspace -> drawIcon(canvas, Icons.backspace, rect.centerX(), rect.centerY(), size, color)
            // Espaço estreito (teclado numérico): ícone no lugar do nome do idioma.
            is KeyAction.Space -> {
                if (rect.width() >= rect.height() * 2.5f) return false
                drawIcon(canvas, Icons.space, rect.centerX(), rect.centerY(), size, color)
            }
            is KeyAction.SwitchIme -> drawIcon(canvas, Icons.globe, rect.centerX(), rect.centerY(), dp(20f), color)
            is KeyAction.Enter -> {
                if (enterLabel != null) return false
                val path = when (enterIcon) {
                    EnterIcon.RETURN -> Icons.enter
                    EnterIcon.SEARCH -> Icons.search
                    EnterIcon.SEND -> Icons.send
                    EnterIcon.DONE -> Icons.check
                    EnterIcon.PREVIOUS -> Icons.arrowLeft
                    EnterIcon.GO, EnterIcon.NEXT -> Icons.arrowRight
                }
                drawIcon(canvas, path, rect.centerX(), rect.centerY(), size, color)
            }
            else -> return false
        }
        return true
    }

    private fun drawIcon(canvas: Canvas, path: Path, cx: Float, cy: Float, size: Float, color: Int, filled: Boolean = false) {
        iconPaint.color = color
        iconPaint.style = if (filled) Paint.Style.FILL_AND_STROKE else Paint.Style.STROKE
        iconPaint.strokeWidth = 1.9f
        canvas.save()
        canvas.translate(cx - size / 2f, cy - size / 2f)
        canvas.scale(size / 24f, size / 24f)
        canvas.drawPath(path, iconPaint)
        canvas.restore()
    }

    /** Balão com a letra logo acima da tecla. */
    private fun drawPreview(canvas: Canvas, slot: Slot, label: String, radius: Float) {
        if (label.isEmpty()) return
        val r = slot.rect
        val w = r.width() + dp(8f)
        val h = r.height() * 1.1f
        var left = r.centerX() - w / 2f
        left = left.coerceIn(dp(1f), width - dp(1f) - w)
        val bottom = r.top - dp(4f)
        val box = RectF(left, bottom - h, left + w, bottom)
        shadowPaint.color = palette.keyShadow
        val sh = RectF(box); sh.offset(0f, dp(2f))
        canvas.drawRoundRect(sh, radius * 1.4f, radius * 1.4f, shadowPaint)
        popupPaint.color = palette.popupBackground
        canvas.drawRoundRect(box, radius * 1.4f, radius * 1.4f, popupPaint)
        labelPaint.color = palette.popupText
        labelPaint.textSize = sp(theme.labelSp * 1.35f)
        labelPaint.isFakeBoldText = theme.keyTextBold && typeface == null
        val fm = labelPaint.fontMetrics
        canvas.drawText(label, box.centerX(), box.centerY() - (fm.ascent + fm.descent) / 2f, labelPaint)
    }

    private fun drawPopup(canvas: Canvas, radius: Float) {
        shadowPaint.color = palette.keyShadow
        val sh = RectF(popupRect); sh.offset(0f, dp(2f))
        canvas.drawRoundRect(sh, radius, radius, shadowPaint)
        popupPaint.color = palette.popupBackground
        canvas.drawRoundRect(popupRect, radius, radius, popupPaint)
        val cell = popupRect.width() / popupItems.size
        labelPaint.textSize = sp(theme.labelSp)
        labelPaint.isFakeBoldText = false
        for ((i, item) in popupItems.withIndex()) {
            val cx = popupRect.left + cell * i + cell / 2f
            if (i == popupSelected) {
                popupPaint.color = palette.popupSelected
                val r = RectF(popupRect.left + cell * i + dp(2f), popupRect.top + dp(2f), popupRect.left + cell * (i + 1) - dp(2f), popupRect.bottom - dp(2f))
                canvas.drawRoundRect(r, radius, radius, popupPaint)
                labelPaint.color = palette.activeKeyText
            } else {
                labelPaint.color = palette.popupText
            }
            val fm = labelPaint.fontMetrics
            canvas.drawText(item, cx, popupRect.centerY() - (fm.ascent + fm.descent) / 2f, labelPaint)
        }
    }

    /**
     * Barra de cima, por prioridade: painel aberto (voltar + título), aviso, sugestões da palavra
     * em digitação ou, parada, o chip do que foi copiado + emoji, histórico e o app.
     */
    private fun drawToolbar(canvas: Canvas) {
        val pad = dp(6f)
        val btn = toolbarHeight - 2 * pad
        val top = overlayHeight + pad
        for (r in listOf(chipRect, appRect, emojiRect, historyRect, backRect) + suggestionRects) r.setEmpty()

        val title = panelTitle
        if (title == null && notice == null && suggestions.isNotEmpty()) {
            drawSuggestions(canvas, top, btn)
            return
        }

        appRect.set(width - sidePadding - pad - btn, top, width - sidePadding - pad, top + btn)
        drawButtonBg(canvas, appRect, ToolbarTarget.APP)
        appIcon?.let {
            val s = dp(22f).toInt()
            val l = (appRect.centerX() - s / 2f).toInt()
            val t = (appRect.centerY() - s / 2f).toInt()
            it.setBounds(l, t, l + s, t + s)
            it.setTint(palette.functionKeyText)
            it.draw(canvas)
        }

        if (title != null) {
            backRect.set(sidePadding + pad, top, sidePadding + pad + btn, top + btn)
            drawButtonBg(canvas, backRect, ToolbarTarget.BACK)
            drawIcon(canvas, Icons.arrowLeft, backRect.centerX(), backRect.centerY(), dp(22f), palette.functionKeyText)
            drawToolbarText(canvas, title, backRect.right + dp(8f), appRect.left - pad, palette.keyText)
            return
        }

        notice?.let {
            drawToolbarText(canvas, it, sidePadding + pad + dp(6f), appRect.left - pad, palette.hint)
            return
        }

        historyRect.set(appRect.left - pad - btn, top, appRect.left - pad, top + btn)
        drawButtonBg(canvas, historyRect, ToolbarTarget.HISTORY)
        drawIcon(canvas, Icons.history, historyRect.centerX(), historyRect.centerY(), dp(22f), palette.functionKeyText)
        emojiRect.set(historyRect.left - pad - btn, top, historyRect.left - pad, top + btn)
        drawButtonBg(canvas, emojiRect, ToolbarTarget.EMOJI)
        drawIcon(canvas, Icons.smiley, emojiRect.centerX(), emojiRect.centerY(), dp(22f), palette.functionKeyText)

        val c = clip ?: return
        val maxW = emojiRect.left - pad - sidePadding - pad
        chipTextPaint.textSize = sp(14f)
        chipTextPaint.isFakeBoldText = false
        chipTextPaint.color = palette.keyText
        val img = c.image
        // Imagem: miniatura no lugar do ícone, quase da altura do chip.
        val iconSize = if (img != null) btn - dp(8f) else dp(16f)
        val iconLeft = if (img != null) dp(4f) else dp(12f)
        val inner = iconLeft + iconSize + dp(8f) + dp(14f)
        if (maxW <= inner) return
        val shown = if (c.masked) "••••••••" else c.text.replace(Regex("\\s+"), " ").trim()
        val text = TextUtils.ellipsize(shown, chipTextPaint, maxW - inner, TextUtils.TruncateAt.END).toString()
        val w = inner + chipTextPaint.measureText(text)
        chipRect.set(sidePadding + pad, top, sidePadding + pad + w, top + btn)
        popupPaint.color = if (toolbarPressed == ToolbarTarget.CLIP) palette.keyPressed else palette.key
        canvas.drawRoundRect(chipRect, btn / 2f, btn / 2f, popupPaint)
        if (img != null) {
            val cy = chipRect.centerY()
            drawThumb(canvas, img, RectF(chipRect.left + iconLeft, cy - iconSize / 2f, chipRect.left + iconLeft + iconSize, cy + iconSize / 2f))
        } else {
            drawIcon(canvas, Icons.clipboard, chipRect.left + iconLeft + iconSize / 2f, chipRect.centerY(), iconSize, palette.hint)
        }
        val fm = chipTextPaint.fontMetrics
        canvas.drawText(text, chipRect.left + iconLeft + iconSize + dp(8f), chipRect.centerY() - (fm.ascent + fm.descent) / 2f, chipTextPaint)
    }

    private fun drawButtonBg(canvas: Canvas, r: RectF, target: ToolbarTarget) {
        if (toolbarPressed != target) return
        popupPaint.color = palette.functionKey
        canvas.drawRoundRect(r, r.height() / 2f, r.height() / 2f, popupPaint)
    }

    private fun drawToolbarText(canvas: Canvas, text: String, left: Float, right: Float, color: Int) {
        chipTextPaint.textSize = sp(14f)
        chipTextPaint.isFakeBoldText = false
        chipTextPaint.color = color
        val shown = TextUtils.ellipsize(text, chipTextPaint, max(0f, right - left), TextUtils.TruncateAt.END).toString()
        val fm = chipTextPaint.fontMetrics
        canvas.drawText(shown, left, overlayHeight + toolbarHeight / 2f - (fm.ascent + fm.descent) / 2f, chipTextPaint)
    }

    private fun drawSuggestions(canvas: Canvas, top: Float, btn: Float) {
        val n = suggestions.size.coerceAtMost(3)
        val left = sidePadding
        val cell = (width - 2 * sidePadding) / 3f
        chipTextPaint.textSize = sp(16f)
        for (i in 0 until n) {
            val r = suggestionRects[i]
            r.set(left + cell * i + dp(2f), top, left + cell * (i + 1) - dp(2f), top + btn)
            if (toolbarPressed == ToolbarTarget.SUGGESTION && pressedSuggestion == i) {
                popupPaint.color = palette.functionKey
                canvas.drawRoundRect(r, r.height() / 2f, r.height() / 2f, popupPaint)
            }
            chipTextPaint.isFakeBoldText = i == suggestionPick
            chipTextPaint.color = palette.keyText
            val text = TextUtils.ellipsize(suggestions[i], chipTextPaint, r.width() - dp(12f), TextUtils.TruncateAt.END).toString()
            val tw = chipTextPaint.measureText(text)
            val fm = chipTextPaint.fontMetrics
            canvas.drawText(text, r.centerX() - tw / 2f, r.centerY() - (fm.ascent + fm.descent) / 2f, chipTextPaint)
            if (i > 0) {
                popupPaint.color = palette.hint
                popupPaint.alpha = 70
                canvas.drawRect(r.left - dp(2.5f), r.top + dp(8f), r.left - dp(1.5f), r.bottom - dp(8f), popupPaint)
                popupPaint.alpha = 255
            }
        }
        chipTextPaint.isFakeBoldText = false
    }

    /** Miniatura recortada ao centro e redonda, no lugar do ícone do chip. */
    private fun drawThumb(canvas: Canvas, bmp: Bitmap, dst: RectF) {
        val side = minOf(bmp.width, bmp.height)
        val src = Rect((bmp.width - side) / 2, (bmp.height - side) / 2, (bmp.width + side) / 2, (bmp.height + side) / 2)
        val round = Path().apply { addRoundRect(dst, dst.height() / 2f, dst.height() / 2f, Path.Direction.CW) }
        canvas.save()
        canvas.clipPath(round)
        canvas.drawBitmap(bmp, src, dst, thumbPaint)
        canvas.restore()
    }

    // ---- toque ----

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                activePointer = event.getPointerId(0)
                if (event.y < overlayHeight + toolbarHeight) {
                    toolbarPressed = toolbarTargetAt(event.x, event.y)
                    if (toolbarPressed != null) { performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP); invalidate() }
                } else if (panelTitle == null) {
                    press(event.x, event.y)
                }
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                if (toolbarPressed != null || cursorMode) return true
                // Segundo dedo: conclui a tecla atual (digitação rápida) e começa a nova.
                release(commit = true)
                val idx = event.actionIndex
                activePointer = event.getPointerId(idx)
                press(event.getX(idx), event.getY(idx))
            }
            MotionEvent.ACTION_MOVE -> {
                val idx = event.findPointerIndex(activePointer)
                if (idx >= 0) {
                    val x = event.getX(idx)
                    when {
                        popupSlot != null -> updatePopupSelection(x)
                        pressed?.key?.action is KeyAction.Space -> trackSpaceSwipe(x)
                    }
                }
            }
            MotionEvent.ACTION_UP -> {
                val target = toolbarPressed
                if (target != null) {
                    toolbarPressed = null
                    invalidate()
                    if (rectOf(target).contains(event.x, event.y)) onToolbarTap(target)
                } else {
                    release(commit = true)
                }
            }
            MotionEvent.ACTION_POINTER_UP -> {
                if (event.getPointerId(event.actionIndex) == activePointer) release(commit = true)
            }
            MotionEvent.ACTION_CANCEL -> { toolbarPressed = null; release(commit = false) }
        }
        return true
    }

    private fun toolbarTargetAt(x: Float, y: Float): ToolbarTarget? {
        suggestionRects.indexOfFirst { !it.isEmpty && it.contains(x, y) }.takeIf { it >= 0 }?.let {
            pressedSuggestion = it
            return ToolbarTarget.SUGGESTION
        }
        return listOf(ToolbarTarget.APP, ToolbarTarget.EMOJI, ToolbarTarget.HISTORY, ToolbarTarget.BACK, ToolbarTarget.CLIP)
            .firstOrNull { rectOf(it).let { r -> !r.isEmpty && r.contains(x, y) } }
    }

    private fun rectOf(t: ToolbarTarget): RectF = when (t) {
        ToolbarTarget.CLIP -> chipRect
        ToolbarTarget.APP -> appRect
        ToolbarTarget.EMOJI -> emojiRect
        ToolbarTarget.HISTORY -> historyRect
        ToolbarTarget.BACK -> backRect
        ToolbarTarget.SUGGESTION -> suggestionRects.getOrNull(pressedSuggestion) ?: RectF()
    }

    private fun onToolbarTap(t: ToolbarTarget) {
        when (t) {
            ToolbarTarget.CLIP -> onPasteClip?.invoke()
            ToolbarTarget.APP -> onOpenApp?.invoke()
            ToolbarTarget.EMOJI -> onToolbarAction?.invoke(ToolbarAction.EMOJI)
            ToolbarTarget.HISTORY -> onToolbarAction?.invoke(ToolbarAction.HISTORY)
            ToolbarTarget.BACK -> onToolbarAction?.invoke(ToolbarAction.CLOSE_PANEL)
            ToolbarTarget.SUGGESTION -> suggestions.getOrNull(pressedSuggestion)?.let { onSuggestion?.invoke(it) }
        }
    }

    private fun trackSpaceSwipe(x: Float) {
        val step = dp(10f)
        if (!cursorMode) {
            if (abs(x - spaceStartX) < dp(16f)) return
            cursorMode = true
            cursorLastX = x
            handler.removeCallbacksAndMessages(null)
            invalidate()
            return
        }
        val steps = ((x - cursorLastX) / step).toInt()
        if (steps != 0) {
            cursorLastX += steps * step
            performHapticFeedback(HapticFeedbackConstants.CLOCK_TICK)
            listener?.invoke(KeyAction.MoveCursor(steps))
        }
    }

    private fun slotAt(x: Float, y: Float): Slot? {
        slots.firstOrNull { it.rect.contains(x, y) }?.let { return it }
        // Toque no vão entre teclas: pega a mais próxima na mesma linha.
        return slots.filter { y >= it.rect.top - rowGapPx / 2 && y <= it.rect.bottom + rowGapPx / 2 }
            .minByOrNull { minOf(abs(x - it.rect.left), abs(x - it.rect.right)) }
    }

    private fun press(x: Float, y: Float) {
        val slot = slotAt(x, y) ?: return
        removeCallbacks(hideLinger)
        lingerSlot = null
        pressed = slot
        longPressFired = false
        cursorMode = false
        repeatCount = 0
        spaceStartX = x
        performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
        handler.removeCallbacksAndMessages(null)
        when {
            slot.key.action is KeyAction.Backspace -> handler.postDelayed(repeatRunnable, repeatStartMs)
            slot.key.alternates.isNotEmpty() -> handler.postDelayed(longPressRunnable, longPressMs)
        }
        invalidate()
    }

    private fun onLongPress() {
        val slot = pressed ?: return
        if (slot.key.alternates.isEmpty()) return
        longPressFired = true
        performHapticFeedback(HapticFeedbackConstants.LONG_PRESS)
        val base = labelFor(slot.key)
        // "ß".uppercase() vira "SS": só troca quando a maiúscula é uma letra só.
        val alts = slot.key.alternates.map { a ->
            if (shift == ShiftState.OFF) a else a.uppercase().takeIf { it.length == a.length } ?: a
        }
        // Com número na tecla, ele vem primeiro: segurar e soltar já digita o número.
        popupItems = (if (slot.key.hint != null) alts + base else listOf(base) + alts).distinct()
        popupSlot = slot
        popupSelected = 0
        val cell = max(slot.rect.width(), dp(40f))
        val w = cell * popupItems.size
        var left = slot.rect.centerX() - cell / 2f
        left = left.coerceIn(sidePadding, max(sidePadding, width - sidePadding - w))
        val h = slot.rect.height()
        val top = max(dp(2f), slot.rect.top - h - dp(4f))
        popupRect.set(left, top, left + w, top + h)
        invalidate()
    }

    private fun updatePopupSelection(x: Float) {
        val cell = popupRect.width() / popupItems.size
        val i = ((x - popupRect.left) / cell).toInt().coerceIn(0, popupItems.size - 1)
        if (i != popupSelected) { popupSelected = i; invalidate() }
    }

    private fun release(commit: Boolean) {
        handler.removeCallbacksAndMessages(null)
        val slot = pressed
        pressed = null
        val popup = popupSlot
        popupSlot = null
        val wasCursor = cursorMode
        cursorMode = false
        if (slot != null && commit && !wasCursor && popup == null && !longPressFired && slot.key.action is KeyAction.Text) {
            lingerSlot = slot
            lingerLabel = labelFor(slot.key)
            postDelayed(hideLinger, lingerMs)
        }
        invalidate()
        if (slot == null || !commit || wasCursor) return
        if (popup != null) {
            popupItems.getOrNull(popupSelected)?.let { listener?.invoke(KeyAction.Text(it, it)) }
            return
        }
        if (longPressFired) return
        when (val a = slot.key.action) {
            is KeyAction.Backspace -> {
                // Se o auto-repeat já apagou, o toque curto não apaga de novo.
                if (repeatCount == 0) listener?.invoke(KeyAction.Backspace)
                repeatCount = 0
            }
            is KeyAction.Shift -> {
                val now = System.currentTimeMillis()
                val doubleTap = now - lastShiftTapAt < ViewConfiguration.getDoubleTapTimeout()
                lastShiftTapAt = now
                shift = when {
                    doubleTap && shift == ShiftState.ON -> ShiftState.LOCKED
                    shift == ShiftState.OFF -> ShiftState.ON
                    else -> ShiftState.OFF
                }
                listener?.invoke(KeyAction.Shift)
            }
            else -> listener?.invoke(a)
        }
    }
}

/** Ícones em grade 24×24 (traço 1,9), no estilo dos ícones do app. */
internal object Icons {
    val shift = Path().apply {
        moveTo(12f, 4f); lineTo(4f, 12.5f); lineTo(8.5f, 12.5f); lineTo(8.5f, 18.5f)
        lineTo(15.5f, 18.5f); lineTo(15.5f, 12.5f); lineTo(20f, 12.5f); close()
    }
    val capsBar = Path().apply { moveTo(8.5f, 21.5f); lineTo(15.5f, 21.5f) }
    val backspace = Path().apply {
        moveTo(8f, 5f); lineTo(21f, 5f); lineTo(21f, 19f); lineTo(8f, 19f); lineTo(2.5f, 12f); close()
        moveTo(11.5f, 9.5f); lineTo(16.5f, 14.5f)
        moveTo(16.5f, 9.5f); lineTo(11.5f, 14.5f)
    }
    val enter = Path().apply {
        moveTo(19f, 5.5f); lineTo(19f, 12f); quadTo(19f, 14.5f, 16.5f, 14.5f); lineTo(5f, 14.5f)
        moveTo(9f, 10.5f); lineTo(5f, 14.5f); lineTo(9f, 18.5f)
    }
    val arrowRight = Path().apply {
        moveTo(4.5f, 12f); lineTo(19f, 12f)
        moveTo(13.5f, 6.5f); lineTo(19f, 12f); lineTo(13.5f, 17.5f)
    }
    val arrowLeft = Path().apply {
        moveTo(19.5f, 12f); lineTo(5f, 12f)
        moveTo(10.5f, 6.5f); lineTo(5f, 12f); lineTo(10.5f, 17.5f)
    }
    val search = Path().apply {
        addCircle(10.5f, 10.5f, 6f, Path.Direction.CW)
        moveTo(15f, 15f); lineTo(20f, 20f)
    }
    val send = Path().apply {
        moveTo(3.5f, 11f); lineTo(20.5f, 4f); lineTo(14f, 20.5f); lineTo(11f, 13f); close()
        moveTo(11f, 13f); lineTo(20.5f, 4f)
    }
    val check = Path().apply { moveTo(5f, 12.5f); lineTo(10f, 17.5f); lineTo(19f, 7f) }
    val globe = Path().apply {
        addCircle(12f, 12f, 9f, Path.Direction.CW)
        addOval(RectF(8f, 3f, 16f, 21f), Path.Direction.CW)
        moveTo(3f, 12f); lineTo(21f, 12f)
    }
    val space = Path().apply { moveTo(5f, 12.5f); lineTo(5f, 16f); lineTo(19f, 16f); lineTo(19f, 12.5f) }
    val smiley = Path().apply {
        addCircle(12f, 12f, 9f, Path.Direction.CW)
        addCircle(9f, 10f, 0.6f, Path.Direction.CW)
        addCircle(15f, 10f, 0.6f, Path.Direction.CW)
        moveTo(8.2f, 14f); quadTo(12f, 18f, 15.8f, 14f)
    }
    val history = Path().apply {
        moveTo(4.6f, 8.5f); arcTo(RectF(3.5f, 3.5f, 20.5f, 20.5f), 200f, 320f, false)
        moveTo(3.8f, 4.5f); lineTo(4.6f, 8.5f); lineTo(8.6f, 7.6f)
        moveTo(12f, 7.5f); lineTo(12f, 12f); lineTo(15f, 14f)
    }
    val clipboard = Path().apply {
        addRoundRect(RectF(5.5f, 4.5f, 18.5f, 21f), 2f, 2f, Path.Direction.CW)
        addRoundRect(RectF(9f, 2.5f, 15f, 6.5f), 1f, 1f, Path.Direction.CW)
    }
}
