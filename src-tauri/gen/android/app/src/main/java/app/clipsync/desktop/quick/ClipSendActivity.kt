package app.clipsync.desktop.quick

import android.app.Activity
import android.content.ClipDescription
import android.content.ClipboardManager
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.widget.Toast
import app.clipsync.desktop.MainActivity
import app.clipsync.desktop.R
import app.clipsync.desktop.ime.CoreBridge
import app.clipsync.desktop.share.ShareBridge
import org.json.JSONArray
import org.json.JSONObject

/**
 * Activity invisível por trás do bloco, do widget e de "Selecionar texto → ClipSync": ganha foco por
 * um instante (única forma de ler o clipboard no Android 10+ sem ser o teclado), entrega o texto e fecha.
 * Núcleo vivo: envio explícito pela JNI + Toast com o resultado. Núcleo morto: abre o app na folha
 * "Enviar para" (mesmo caminho do "Compartilhar"). Ver docs/features/04-android-bloco-widget-texto.md.
 */
class ClipSendActivity : Activity() {
    private val main = Handler(Looper.getMainLooper())
    private var handled = false
    private val giveUp = Runnable {
        if (!handled) {
            handled = true
            toastAndFinish(getString(R.string.quick_read_failed))
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        start(intent)
    }

    // singleInstance: segundo toque com a Activity ainda viva reaproveita esta instância.
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        start(intent)
    }

    private fun start(intent: Intent?) {
        handled = false
        main.removeCallbacks(giveUp)
        when (intent?.action) {
            Intent.ACTION_PROCESS_TEXT -> {
                // Texto vem no Intent: não precisa de foco. Nunca devolvemos resultado (o texto selecionado fica como está).
                handled = true
                deliver(intent.getCharSequenceExtra(Intent.EXTRA_PROCESS_TEXT)?.toString().orEmpty(), false)
            }
            ACTION_SEND_CLIPBOARD -> {
                if (!authorized(intent)) return finishQuietly()
                // Antes de `onWindowFocusChanged(true)` a leitura volta null.
                if (hasWindowFocus()) readClipboard() else main.postDelayed(giveUp, FOCUS_TIMEOUT_MS)
            }
            ACTION_COPY_LATEST -> {
                if (!authorized(intent)) return finishQuietly()
                handled = true
                copyLatest()
            }
            else -> finishQuietly()
        }
    }

    override fun onWindowFocusChanged(hasFocus: Boolean) {
        super.onWindowFocusChanged(hasFocus)
        if (hasFocus && !handled && intent?.action == ACTION_SEND_CLIPBOARD) readClipboard()
    }

    private fun readClipboard() {
        handled = true
        main.removeCallbacks(giveUp)
        val clip = runCatching { (getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager).primaryClip }.getOrNull()
        if (clip == null || clip.itemCount == 0) return toastAndFinish(getString(R.string.quick_empty))
        val desc = clip.description
        val isText = desc.hasMimeType(ClipDescription.MIMETYPE_TEXT_PLAIN) || desc.hasMimeType(ClipDescription.MIMETYPE_TEXT_HTML)
        if (!isText) {
            if (desc.hasMimeType("image/*")) {
                // O poll da MainActivity ao ganhar foco trata a imagem pelas regras de sync automático.
                toast(getString(R.string.quick_image))
                return openMainAndFinish()
            }
            return toastAndFinish(getString(R.string.quick_empty))
        }
        val item = clip.getItemAt(0)
        val text = (item.text ?: item.coerceToText(this)).toString()
        val sensitive = Build.VERSION.SDK_INT >= 33 && desc.extras?.getBoolean(ClipDescription.EXTRA_IS_SENSITIVE, false) == true
        deliver(text, sensitive)
    }

    private fun deliver(text: String, sensitive: Boolean) {
        if (text.isBlank()) return toastAndFinish(getString(R.string.quick_empty))
        if (!CoreBridge.ready) {
            // Processo criado só para esta Activity: a folha "Enviar para" do app faz o envio.
            ShareBridge.offerText(this, text)
            return openMainAndFinish()
        }
        runOffMain({ CoreBridge.nativeSendNow(text, sensitive) }) { ctx, raw -> resultMessage(ctx, raw) }
        finishQuietly()
    }

    private fun copyLatest() {
        if (!CoreBridge.ready) return openMainAndFinish()
        runOffMain({ CoreBridge.nativeCopyLatest() }) { ctx, ok ->
            ctx.getString(if (ok) R.string.quick_copied else R.string.quick_copy_none)
        }
        finishQuietly()
    }

    /** A JNI bloqueia (reconexão curta, SQLite): roda em thread própria e mostra o Toast com o contexto do app. */
    private fun <T> runOffMain(work: () -> T, message: (Context, T) -> String) {
        val app = applicationContext
        Thread({
            val msg = runCatching { message(app, work()) }.getOrElse { app.getString(R.string.quick_error, it.message ?: "") }
            Handler(Looper.getMainLooper()).post { Toast.makeText(app, msg, Toast.LENGTH_SHORT).show() }
        }, "clipsync-quick").start()
    }

    private fun openMainAndFinish() {
        startActivity(Intent(this, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        finishQuietly()
    }

    private fun toastAndFinish(msg: String) {
        toast(msg)
        finishQuietly()
    }

    private fun toast(msg: String) = Toast.makeText(applicationContext, msg, Toast.LENGTH_SHORT).show()

    private fun finishQuietly() {
        finish()
        @Suppress("DEPRECATION")
        overridePendingTransition(0, 0)
    }

    private fun authorized(intent: Intent) = intent.getStringExtra(EXTRA_TOKEN) == StatusStore.token(this)

    override fun onDestroy() {
        main.removeCallbacks(giveUp)
        super.onDestroy()
    }

    companion object {
        const val ACTION_SEND_CLIPBOARD = "app.clipsync.desktop.SEND_CLIPBOARD"
        const val ACTION_COPY_LATEST = "app.clipsync.desktop.COPY_LATEST"
        private const val EXTRA_TOKEN = "token"
        private const val FOCUS_TIMEOUT_MS = 1500L

        /** Intent explícito (sem intent-filter) com o segredo que prova que veio do próprio ClipSync. */
        fun intentFor(context: Context, action: String): Intent =
            Intent(context, ClipSendActivity::class.java)
                .setAction(action)
                .putExtra(EXTRA_TOKEN, StatusStore.token(context))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

        private fun names(a: JSONArray?): String = (0 until (a?.length() ?: 0)).joinToString(", ") { a!!.optString(it) }

        private fun resultMessage(ctx: Context, raw: String): String {
            val r = JSONObject(raw)
            if (!r.optBoolean("ok")) return ctx.getString(R.string.quick_error, r.optString("error"))
            val sent = names(r.optJSONArray("names"))
            val queued = names(r.optJSONArray("queuedNames"))
            val nSent = r.optInt("sent")
            val nQueued = r.optInt("queued")
            return when {
                nSent > 0 && nQueued > 0 -> ctx.getString(R.string.quick_sent_and_queued, sent, queued)
                nSent > 0 -> ctx.getString(R.string.quick_sent, sent)
                nQueued > 0 -> ctx.getString(R.string.quick_queued, queued)
                else -> ctx.getString(R.string.quick_no_device)
            }
        }
    }
}
