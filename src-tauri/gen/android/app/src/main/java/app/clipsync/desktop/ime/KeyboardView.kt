package app.clipsync.desktop.ime

import android.content.Context
import android.content.res.Configuration
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.graphics.Typeface
import android.os.Handler
import android.os.Build
import android.os.Looper
import android.util.TypedValue
import android.view.HapticFeedbackConstants
import android.view.MotionEvent
import android.view.View
import android.view.ViewConfiguration
import android.view.WindowInsets
import kotlin.math.max

enum class ShiftState { OFF, ON, LOCKED }

/**
 * Renderiza uma [KeyboardLayer] com um [KeyboardTheme] e traduz toques em
 * [KeyAction]. Não conhece o campo de texto: só avisa o [listener].
 *
 * O `android.inputmethodservice.KeyboardView` do SDK é deprecated (API 29),
 * por isso a view é própria. Não registra nem persiste nada do que é digitado.
 */
class KeyboardView(context: Context) : View(context) {

    var listener: ((KeyAction) -> Unit)? = null

    var theme: KeyboardTheme = KeyboardThemes.default
        set(value) { field = value; applyTheme(); requestLayout(); invalidate() }

    var layer: KeyboardLayer = QwertyLayout.layers.getValue(Layers.LETTERS)
        set(value) { field = value; requestLayout(); invalidate() }

    var shift: ShiftState = ShiftState.OFF
        set(value) { field = value; invalidate() }

    /** Rótulo da tecla Enter conforme a ação do campo ("Ir", "Buscar"...). */
    var enterLabel: String = "↵"
        set(value) { field = value; invalidate() }

    private class Slot(val key: Key, val rect: RectF)
    private val slots = ArrayList<Slot>()
    private var palette: Palette = theme.palette(false)
    private var typeface: Typeface? = null
    private val dm get() = resources.displayMetrics
    private fun dp(v: Float) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, dm)
    private fun sp(v: Float) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, v, dm)
    private val sidePadding get() = dp(3f)
    private val topPadding get() = dp(8f)
    /** Barra de navegação/gestos por baixo do teclado (edge-to-edge): vira padding inferior. */
    private var navBottom = 0
    private val bottomPadding get() = dp(6f) + navBottom

    private val keyPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val shadowPaint = Paint(Paint.ANTI_ALIAS_FLAG)
    private val labelPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { textAlign = Paint.Align.CENTER }
    private val hintPaint = Paint(Paint.ANTI_ALIAS_FLAG).apply { textAlign = Paint.Align.RIGHT }
    private val popupPaint = Paint(Paint.ANTI_ALIAS_FLAG)

    private val handler = Handler(Looper.getMainLooper())
    private var activePointer = MotionEvent.INVALID_POINTER_ID
    private var pressed: Slot? = null
    private var longPressFired = false
    private var popupSlot: Slot? = null
    private var popupItems: List<String> = emptyList()
    private var popupSelected = -1
    private val popupRect = RectF()
    private val longPressMs = ViewConfiguration.getLongPressTimeout().toLong()
    private val repeatStartMs = 350L
    private val repeatIntervalMs = 45L
    private var lastShiftTapAt = 0L
    private var repeatedOnce = false

    private val longPressRunnable = Runnable { onLongPress() }
    private val repeatRunnable = object : Runnable {
        override fun run() {
            val s = pressed ?: return
            if (s.key.action is KeyAction.Backspace) {
                repeatedOnce = true
                listener?.invoke(KeyAction.Backspace)
                handler.postDelayed(this, repeatIntervalMs)
            }
        }
    }

    init {
        isClickable = true
        isHapticFeedbackEnabled = true
        applyTheme()
    }

    val isDark: Boolean
        get() = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

    private fun applyTheme() {
        palette = theme.palette(isDark)
        typeface = theme.typeface(context)
        labelPaint.typeface = typeface ?: Typeface.DEFAULT
        hintPaint.typeface = typeface ?: Typeface.DEFAULT
        setBackgroundColor(palette.background)
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
        val h = topPadding + bottomPadding + rows * dp(theme.keyHeightDp) + max(0, rows - 1) * dp(theme.rowGapDp)
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
        val rowGap = dp(theme.rowGapDp)
        val keyH = dp(theme.keyHeightDp)
        val usable = width - 2 * sidePadding
        // Largura de uma unidade: a linha mais larga define (normalmente 10 teclas).
        val maxUnits = layer.rows.maxOf { it.totalWidth }
        val unit = (usable - (maxOf(layer.rows.maxOf { it.keys.size }, 1) - 1) * gap) / maxUnits
        var y = topPadding
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
    }

    override fun onDraw(canvas: Canvas) {
        super.onDraw(canvas)
        val radius = dp(theme.keyRadiusDp)
        val shadowDy = dp(1f)
        for (slot in slots) {
            val key = slot.key
            val isPressed = slot === pressed
            val (bg, fg) = colorsFor(key, isPressed)
            if (theme.hasKeyShadow) {
                shadowPaint.color = palette.keyShadow
                val r = RectF(slot.rect); r.offset(0f, shadowDy)
                canvas.drawRoundRect(r, radius, radius, shadowPaint)
            }
            keyPaint.color = bg
            canvas.drawRoundRect(slot.rect, radius, radius, keyPaint)

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

            if (key.alternates.isNotEmpty() && key.action is KeyAction.Text && !key.isFunction) {
                hintPaint.color = palette.hint
                hintPaint.textSize = sp(10f)
                canvas.drawText(key.alternates.first(), slot.rect.right - dp(5f), slot.rect.top + dp(13f), hintPaint)
            }
        }
        if (popupSlot != null) drawPopup(canvas, radius)
    }

    private fun colorsFor(key: Key, isPressed: Boolean): Pair<Int, Int> = when (key.action) {
        is KeyAction.Enter -> (if (isPressed) palette.accentKeyPressed else palette.accentKey) to palette.accentKeyText
        is KeyAction.Shift -> when {
            shift == ShiftState.LOCKED -> palette.activeKey to palette.activeKeyText
            shift == ShiftState.ON -> palette.key to palette.keyText
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
            is KeyAction.Enter -> enterLabel
            is KeyAction.Space -> ""
            else -> key.label ?: ""
        }
    }

    private fun drawPopup(canvas: Canvas, radius: Float) {
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
                labelPaint.color = palette.accentKeyText
            } else {
                labelPaint.color = palette.popupText
            }
            val fm = labelPaint.fontMetrics
            canvas.drawText(item, cx, popupRect.centerY() - (fm.ascent + fm.descent) / 2f, labelPaint)
        }
    }

    override fun onTouchEvent(event: MotionEvent): Boolean {
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> {
                activePointer = event.getPointerId(0)
                press(event.x, event.y)
            }
            MotionEvent.ACTION_POINTER_DOWN -> {
                // Segundo dedo: conclui a tecla atual (digitação rápida) e começa a nova.
                release(commit = true)
                val idx = event.actionIndex
                activePointer = event.getPointerId(idx)
                press(event.getX(idx), event.getY(idx))
            }
            MotionEvent.ACTION_MOVE -> {
                val idx = event.findPointerIndex(activePointer)
                if (idx >= 0 && popupSlot != null) updatePopupSelection(event.getX(idx))
            }
            MotionEvent.ACTION_UP -> release(commit = true)
            MotionEvent.ACTION_POINTER_UP -> {
                if (event.getPointerId(event.actionIndex) == activePointer) release(commit = true)
            }
            MotionEvent.ACTION_CANCEL -> release(commit = false)
        }
        return true
    }

    private fun slotAt(x: Float, y: Float): Slot? {
        slots.firstOrNull { it.rect.contains(x, y) }?.let { return it }
        // Toque no vão entre teclas: pega a mais próxima na mesma linha.
        return slots.filter { y >= it.rect.top - dp(theme.rowGapDp) / 2 && y <= it.rect.bottom + dp(theme.rowGapDp) / 2 }
            .minByOrNull { minOf(kotlin.math.abs(x - it.rect.left), kotlin.math.abs(x - it.rect.right)) }
    }

    private fun press(x: Float, y: Float) {
        val slot = slotAt(x, y) ?: return
        pressed = slot
        longPressFired = false
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
        popupItems = (listOf(base) + slot.key.alternates.map { if (shift != ShiftState.OFF) it.uppercase() else it }).distinct()
        popupSlot = slot
        popupSelected = 0
        val cell = max(slot.rect.width(), dp(40f))
        val w = cell * popupItems.size
        var left = slot.rect.centerX() - cell / 2f
        left = left.coerceIn(sidePadding, max(sidePadding, width - sidePadding - w))
        val h = slot.rect.height()
        val top = if (slot.rect.top - h - dp(6f) >= 0f) slot.rect.top - h - dp(6f) else slot.rect.top
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
        invalidate()
        if (slot == null || !commit) return
        if (popup != null) {
            popupItems.getOrNull(popupSelected)?.let { listener?.invoke(KeyAction.Text(it, it)) }
            return
        }
        if (longPressFired) return
        when (val a = slot.key.action) {
            is KeyAction.Backspace -> {
                // Se o auto-repeat já apagou, o toque curto não apaga de novo.
                if (!repeatedOnce) listener?.invoke(KeyAction.Backspace)
                repeatedOnce = false
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
