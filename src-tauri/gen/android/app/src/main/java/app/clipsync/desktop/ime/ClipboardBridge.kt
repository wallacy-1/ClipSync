package app.clipsync.desktop.ime

import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.util.Log
import app.clipsync.desktop.BuildConfig
import java.security.MessageDigest
import java.util.concurrent.Executors

/**
 * Único leitor de clipboard do IME. Sendo o teclado padrão, o Android entrega
 * `OnPrimaryClipChangedListener` a este processo mesmo sem foco de janela
 * (docs/ime-probe-clipboard.md). O listener dispara 2–3× por cópia, por isso o
 * dedupe por hash em [DEDUPE_WINDOW_MS]. Com o núcleo vivo entrega via
 * `CoreBridge.nativeOnClip`; com ele morto, guarda no [ImeInbox].
 * Nunca registra o conteúdo (log só de hash/tamanho, em debug).
 */
object ClipboardBridge {
    private const val TAG = "ClipSyncIME"

    /** Mesmo hash chegando de novo dentro desta janela é o mesmo evento, não outra cópia. */
    const val DEDUPE_WINDOW_MS = 3_000L

    private var cm: ClipboardManager? = null
    private var appContext: Context? = null
    private val worker = Executors.newSingleThreadExecutor { r -> Thread(r, "clipsync-ime-clip") }
    private var lastDeliveredHash: String? = null
    private var lastDeliveredAt = 0L

    @Volatile
    var imeShown = false

    private val listener = ClipboardManager.OnPrimaryClipChangedListener { onChanged() }

    fun start(context: Context) {
        if (cm != null) return
        val ctx = context.applicationContext
        val m = ctx.getSystemService(Context.CLIPBOARD_SERVICE) as? ClipboardManager ?: return
        appContext = ctx
        cm = m
        m.addPrimaryClipChangedListener(listener)
        debug("listener registered")
    }

    fun stop() {
        cm?.removePrimaryClipChangedListener(listener)
        cm = null
        debug("listener removed")
    }

    private fun onChanged() {
        val m = cm ?: return
        val ctx = appContext ?: return
        // Extrai o mínimo na thread principal; hash, JNI e disco vão para o worker.
        val clip = runCatching { m.primaryClip }.getOrNull() ?: return
        val desc = clip.description
        val isText = desc.hasMimeType(ClipDescription.MIMETYPE_TEXT_PLAIN) ||
            desc.hasMimeType(ClipDescription.MIMETYPE_TEXT_HTML)
        if (!isText || clip.itemCount == 0) return
        val text = clip.getItemAt(0).coerceToText(ctx)?.toString() ?: return
        if (text.isBlank()) return
        val sensitive = desc.extras?.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE, false) ?: false
        val shown = imeShown
        worker.execute { deliver(ctx, text, sensitive, shown) }
    }

    private fun deliver(ctx: Context, text: String, sensitive: Boolean, shown: Boolean) {
        val hash = sha256(text)
        val now = System.currentTimeMillis()
        if (hash == lastDeliveredHash && now - lastDeliveredAt < DEDUPE_WINDOW_MS) {
            debug("event dup hash=${hash.take(8)}")
            return
        }
        lastDeliveredHash = hash
        lastDeliveredAt = now

        val route = when {
            CoreBridge.ready && runCatching { CoreBridge.nativeOnClip(text, sensitive) }.getOrDefault(false) -> "core"
            ImeInbox.put(ctx, text, hash, sensitive) -> "inbox"
            else -> "dropped"
        }
        debug("event hash=${hash.take(8)} len=${text.length} mime=text/plain sensitive=$sensitive imeShown=$shown route=$route")
    }

    private fun sha256(s: String): String {
        val md = MessageDigest.getInstance("SHA-256").digest(s.toByteArray(Charsets.UTF_8))
        val sb = StringBuilder(64)
        for (b in md) sb.append("%02x".format(b))
        return sb.toString()
    }

    /** Só em builds debug; nunca recebe texto. */
    fun debug(msg: String) {
        if (BuildConfig.DEBUG) Log.d(TAG, "CLIPBOARD_IME $msg")
    }
}
