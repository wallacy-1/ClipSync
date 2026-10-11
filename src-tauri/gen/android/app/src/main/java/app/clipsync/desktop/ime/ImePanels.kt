package app.clipsync.desktop.ime

import android.content.Context
import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Typeface
import android.graphics.drawable.GradientDrawable
import android.text.TextUtils
import android.util.TypedValue
import android.view.Gravity
import android.view.HapticFeedbackConstants
import android.view.View
import android.view.ViewGroup
import android.widget.AbsListView
import android.widget.BaseAdapter
import android.widget.FrameLayout
import android.widget.GridView
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.TextView
import app.clipsync.desktop.R
import org.json.JSONArray

private fun Context.dp(v: Float) = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, v, resources.displayMetrics)

private fun rounded(color: Int, radius: Float) = GradientDrawable().apply {
    setColor(color)
    cornerRadius = radius
}

/** Ícone vetorial de [Icons] (grade 24×24), com a mesma espessura de traço do teclado. */
private class IconView(context: Context, private val path: Path, private val color: Int, private val sizeDp: Float = 22f) : View(context) {
    private val paint = Paint(Paint.ANTI_ALIAS_FLAG).apply {
        style = Paint.Style.STROKE
        strokeWidth = 1.9f
        strokeCap = Paint.Cap.ROUND
        strokeJoin = Paint.Join.ROUND
        this.color = this@IconView.color
    }

    override fun onDraw(canvas: Canvas) {
        val size = context.dp(sizeDp)
        canvas.save()
        canvas.translate((width - size) / 2f, (height - size) / 2f)
        canvas.scale(size / 24f, size / 24f)
        canvas.drawPath(path, paint)
        canvas.restore()
    }
}

/**
 * Painel de emoji: grade rolável da categoria escolhida e, embaixo, ABC, as abas e o apagar.
 * A aba "recentes" só aparece quando há algum.
 */
class EmojiPanel(
    context: Context,
    private val palette: Palette,
    recent: List<String>,
    private val onEmoji: (String) -> Unit,
    onBackspace: () -> Unit,
    onAbc: () -> Unit,
) : LinearLayout(context) {

    private val tabs = ArrayList<Pair<View, List<String>>>()
    private val grid = GridView(context)
    private var items: List<String> = emptyList()

    private val adapter = object : BaseAdapter() {
        override fun getCount() = items.size
        override fun getItem(position: Int) = items[position]
        override fun getItemId(position: Int) = position.toLong()
        override fun getView(position: Int, convertView: View?, parent: ViewGroup?): View {
            val tv = (convertView as? TextView) ?: TextView(context).apply {
                gravity = Gravity.CENTER
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 26f)
                layoutParams = AbsListView.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, context.dp(48f).toInt())
            }
            tv.text = items[position]
            return tv
        }
    }

    init {
        orientation = VERTICAL
        setBackgroundColor(palette.background)
        grid.numColumns = GridView.AUTO_FIT
        grid.columnWidth = context.dp(46f).toInt()
        grid.stretchMode = GridView.STRETCH_COLUMN_WIDTH
        grid.selector = rounded(palette.keyPressed, context.dp(10f))
        grid.adapter = adapter
        grid.setOnItemClickListener { v, _, pos, _ ->
            v.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
            onEmoji(items[pos])
        }
        addView(grid, LayoutParams(LayoutParams.MATCH_PARENT, 0, 1f))

        val bar = LinearLayout(context).apply {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            val p = context.dp(4f).toInt()
            setPadding(p, p, p, p)
        }
        val h = context.dp(40f).toInt()
        bar.addView(TextView(context).apply {
            text = "ABC"
            gravity = Gravity.CENTER
            setTextColor(palette.functionKeyText)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            background = rounded(palette.functionKey, h / 2f)
            setOnClickListener { onAbc() }
        }, LayoutParams(context.dp(56f).toInt(), h))

        val tabRow = LinearLayout(context).apply { orientation = HORIZONTAL }
        if (recent.isNotEmpty()) addTab(tabRow, IconView(context, Icons.history, palette.functionKeyText, 20f), recent)
        for (c in EmojiData.categories) {
            addTab(tabRow, TextView(context).apply {
                text = c.icon
                gravity = Gravity.CENTER
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 18f)
            }, c.emojis)
        }
        val scroller = android.widget.HorizontalScrollView(context).apply {
            isHorizontalScrollBarEnabled = false
            addView(tabRow)
        }
        bar.addView(scroller, LayoutParams(0, h, 1f))

        bar.addView(IconView(context, Icons.backspace, palette.functionKeyText).apply {
            background = rounded(palette.functionKey, h / 2f)
            setOnClickListener {
                it.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                onBackspace()
            }
        }, LayoutParams(context.dp(56f).toInt(), h))
        addView(bar, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))

        select(0)
    }

    private fun addTab(row: LinearLayout, view: View, emojis: List<String>) {
        val i = tabs.size
        view.setOnClickListener { select(i) }
        tabs += view to emojis
        row.addView(view, LayoutParams(context.dp(44f).toInt(), context.dp(40f).toInt()))
    }

    private fun select(i: Int) {
        for ((j, t) in tabs.withIndex()) {
            t.first.background = if (j == i) rounded(palette.functionKey, context.dp(20f)) else null
        }
        items = tabs[i].second
        adapter.notifyDataSetChanged()
        grid.setSelection(0)
    }
}

/** Item do histórico como vem de `CoreBridge.nativeHistory`. */
class HistoryItem(
    val id: String,
    val type: String,
    val text: String,
    val sensitive: Boolean,
    val pinned: Boolean,
    val path: String?,
    val thumb: String?,
) {
    val isImage get() = type == "image"

    companion object {
        fun parse(json: String): List<HistoryItem> = runCatching {
            val arr = JSONArray(json)
            (0 until arr.length()).map { i ->
                val o = arr.getJSONObject(i)
                HistoryItem(
                    id = o.optString("id"),
                    type = o.optString("type"),
                    text = o.optString("text"),
                    sensitive = o.optBoolean("sensitive"),
                    pinned = o.optBoolean("pinned"),
                    path = o.optString("path").takeIf { it.isNotEmpty() && it != "null" },
                    thumb = o.optString("thumb").takeIf { it.isNotEmpty() && it != "null" },
                )
            }
        }.getOrDefault(emptyList())
    }
}

/**
 * Painel do histórico: lista rolável, o toque cola. [partial] = núcleo fora: só o que o teclado
 * guardou nesse meio-tempo, com aviso para abrir o app.
 */
class HistoryPanel(
    context: Context,
    private val palette: Palette,
    typeface: Typeface,
    items: List<HistoryItem>,
    partial: Boolean,
    onPick: (HistoryItem) -> Unit,
    onOpenApp: () -> Unit,
) : FrameLayout(context) {

    private val thumbs = HashMap<String, Bitmap?>()

    init {
        setBackgroundColor(palette.background)
        if (items.isEmpty()) {
            val box = LinearLayout(context).apply {
                orientation = LinearLayout.VERTICAL
                gravity = Gravity.CENTER
            }
            box.addView(TextView(context).apply {
                setText(if (partial) R.string.ime_history_unavailable else R.string.ime_history_empty)
                setTextColor(palette.hint)
                setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
                this.typeface = typeface
                gravity = Gravity.CENTER
            })
            if (partial) {
                val h = context.dp(40f).toInt()
                box.addView(TextView(context).apply {
                    setText(R.string.ime_open_app)
                    setTextColor(palette.accentKeyText)
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
                    this.typeface = typeface
                    gravity = Gravity.CENTER
                    val p = context.dp(20f).toInt()
                    setPadding(p, 0, p, 0)
                    background = rounded(palette.accentKey, h / 2f)
                    setOnClickListener { onOpenApp() }
                }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, h).apply { topMargin = context.dp(14f).toInt() })
            }
            addView(box, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        } else {
            val list = ListView(context).apply {
                divider = null
                selector = rounded(0, 0f)
                val p = context.dp(8f).toInt()
                setPadding(p, p / 2, p, p / 2)
                clipToPadding = false
            }
            if (partial) {
                val banner = LinearLayout(context).apply {
                    orientation = LinearLayout.HORIZONTAL
                    gravity = Gravity.CENTER_VERTICAL
                    val p = context.dp(6f).toInt()
                    setPadding(p, p, p, p)
                }
                banner.addView(TextView(context).apply {
                    setText(R.string.ime_history_partial)
                    setTextColor(palette.hint)
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
                    this.typeface = typeface
                }, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
                val h = context.dp(32f).toInt()
                banner.addView(TextView(context).apply {
                    setText(R.string.ime_open_app)
                    setTextColor(palette.accentKeyText)
                    setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
                    this.typeface = typeface
                    gravity = Gravity.CENTER
                    val p = context.dp(14f).toInt()
                    setPadding(p, 0, p, 0)
                    background = rounded(palette.accentKey, h / 2f)
                    setOnClickListener { onOpenApp() }
                }, LinearLayout.LayoutParams(LinearLayout.LayoutParams.WRAP_CONTENT, h))
                list.addHeaderView(banner, null, false)
            }
            list.adapter = object : BaseAdapter() {
                override fun getCount() = items.size
                override fun getItem(position: Int) = items[position]
                override fun getItemId(position: Int) = position.toLong()
                override fun getView(position: Int, convertView: View?, parent: ViewGroup?): View {
                    val row = (convertView as? Row) ?: Row(context, typeface)
                    row.bind(items[position])
                    return row
                }
            }
            list.setOnItemClickListener { v, _, pos, _ ->
                val item = items.getOrNull(pos - list.headerViewsCount) ?: return@setOnItemClickListener
                v.performHapticFeedback(HapticFeedbackConstants.KEYBOARD_TAP)
                onPick(item)
            }
            addView(list, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        }
    }

    /** Miniatura já pequena (o núcleo grava até 320 px); reduz mais um pouco para a linha. */
    private fun thumbOf(path: String): Bitmap? = thumbs.getOrPut(path) {
        runCatching {
            BitmapFactory.decodeFile(path, BitmapFactory.Options().apply { inSampleSize = 2 })
        }.getOrNull()
    }

    private inner class Row(context: Context, typeface: Typeface) : LinearLayout(context) {
        private val image = ImageView(context).apply {
            scaleType = ImageView.ScaleType.CENTER_CROP
            clipToOutline = true
            background = rounded(palette.functionKey, context.dp(8f))
        }
        private val text = TextView(context).apply {
            setTextColor(palette.keyText)
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            this.typeface = typeface
            maxLines = 2
            ellipsize = TextUtils.TruncateAt.END
        }

        init {
            orientation = HORIZONTAL
            gravity = Gravity.CENTER_VERTICAL
            // Fundo antes do padding: trocar o fundo redefine o padding da view.
            background = android.graphics.drawable.InsetDrawable(rounded(palette.key, context.dp(12f)), 0, context.dp(3f).toInt(), 0, context.dp(3f).toInt())
            val p = context.dp(12f).toInt()
            setPadding(p, p, p, p)
            minimumHeight = context.dp(56f).toInt()
            val s = context.dp(44f).toInt()
            addView(image, LayoutParams(s, s).apply { marginEnd = context.dp(12f).toInt() })
            addView(text, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
        }

        fun bind(item: HistoryItem) {
            val bmp = if (item.isImage) (item.thumb ?: item.path)?.let { thumbOf(it) } else null
            image.visibility = if (item.isImage) View.VISIBLE else View.GONE
            image.setImageBitmap(bmp)
            val body = when {
                item.isImage -> context.getString(R.string.ime_clip_image)
                item.sensitive -> "••••••••"
                else -> item.text.replace(Regex("\\s+"), " ").trim()
            }
            text.text = if (item.pinned) "📌 $body" else body
        }
    }
}
